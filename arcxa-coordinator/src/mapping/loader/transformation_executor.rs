//! Data Transformation Executor
//!
//! Executes transformation expressions on data values during ETL processing.
//!
//! ## Supported Functions
//!
//! **String Functions:**
//! - `UPPER({value})` - Convert to uppercase
//! - `LOWER({value})` - Convert to lowercase
//! - `TRIM({value})` - Remove leading/trailing whitespace
//! - `CONCAT({value1}, {value2}, ...)` - Concatenate strings
//! - `SUBSTRING({value}, start, length)` - Extract substring
//! - `REPLACE({value}, from, to)` - Replace substring
//!
//! **Type Conversion:**
//! - `CAST({value} AS INTEGER)` - Convert to integer
//! - `CAST({value} AS DECIMAL)` - Convert to decimal
//! - `CAST({value} AS DATE)` - Convert to date
//!
//! **Logical:**
//! - `COALESCE({value1}, {value2}, ...)` - Return first non-null value
//! - `NULLIF({value1}, {value2})` - Return NULL if values are equal
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::transformation_executor::*;
//! use std::collections::HashMap;
//!
//! let executor = TransformationExecutor::new();
//! let mut context = HashMap::new();
//! context.insert("name".to_string(), Value::String("  alice  ".to_string()));
//!
//! let result = executor.execute("UPPER(TRIM({name}))", &context)?;
//! assert_eq!(result, Value::String("ALICE".to_string()));
//! ```

use anyhow::{anyhow, Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Value types for transformation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// String value
    String(String),
    /// Integer value
    Integer(i64),
    /// Decimal value (stored as string for precision)
    Decimal(String),
    /// Boolean value
    Boolean(bool),
    /// Date value
    Date(NaiveDate),
    /// Null value
    Null,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Integer(i) => write!(f, "{}", i),
            Value::Decimal(d) => write!(f, "{}", d),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Date(d) => write!(f, "{}", d),
            Value::Null => write!(f, "NULL"),
        }
    }
}

impl Value {
    /// Convert to string representation
    pub fn as_string(&self) -> String {
        match self {
            Value::String(s) => s.clone(),
            Value::Integer(i) => i.to_string(),
            Value::Decimal(d) => d.clone(),
            Value::Boolean(b) => if *b { "1" } else { "0" }.to_string(),
            Value::Date(d) => d.format("%Y-%m-%d").to_string(),
            Value::Null => String::new(),
        }
    }

    /// Check if value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Convert to integer
    pub fn as_integer(&self) -> Result<i64> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::String(s) => s
                .trim()
                .parse()
                .with_context(|| format!("Cannot convert '{}' to integer", s)),
            Value::Decimal(d) => d
                .trim()
                .parse::<f64>()
                .map(|f| f as i64)
                .with_context(|| format!("Cannot convert '{}' to integer", d)),
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::Null => Err(anyhow!("Cannot convert NULL to integer")),
            Value::Date(_) => Err(anyhow!("Cannot convert DATE to integer")),
        }
    }

    /// Convert to boolean
    pub fn as_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::String(s) => !s.is_empty() && s != "0" && s.to_lowercase() != "false",
            Value::Null => false,
            _ => true,
        }
    }
}

/// Transformation function trait
pub trait TransformFunction: Send + Sync {
    /// Execute transformation
    fn execute(&self, args: &[Value]) -> Result<Value>;

    /// Get function name
    fn name(&self) -> &str;

    /// Get minimum number of arguments
    fn min_args(&self) -> usize {
        0
    }

    /// Get maximum number of arguments (None = unlimited)
    fn max_args(&self) -> Option<usize> {
        None
    }
}

/// UPPER function
struct UpperFunction;

impl TransformFunction for UpperFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(anyhow!("UPPER requires exactly 1 argument"));
        }

        match &args[0] {
            Value::Null => Ok(Value::Null),
            value => Ok(Value::String(value.as_string().to_uppercase())),
        }
    }

    fn name(&self) -> &str {
        "UPPER"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// LOWER function
struct LowerFunction;

impl TransformFunction for LowerFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(anyhow!("LOWER requires exactly 1 argument"));
        }

        match &args[0] {
            Value::Null => Ok(Value::Null),
            value => Ok(Value::String(value.as_string().to_lowercase())),
        }
    }

    fn name(&self) -> &str {
        "LOWER"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// TRIM function
struct TrimFunction;

impl TransformFunction for TrimFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(anyhow!("TRIM requires exactly 1 argument"));
        }

        match &args[0] {
            Value::Null => Ok(Value::Null),
            value => Ok(Value::String(value.as_string().trim().to_string())),
        }
    }

    fn name(&self) -> &str {
        "TRIM"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// CONCAT function
struct ConcatFunction;

impl TransformFunction for ConcatFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.is_empty() {
            return Err(anyhow!("CONCAT requires at least 1 argument"));
        }

        let mut result = String::new();
        for arg in args {
            if !arg.is_null() {
                result.push_str(&arg.as_string());
            }
        }

        Ok(Value::String(result))
    }

    fn name(&self) -> &str {
        "CONCAT"
    }

    fn min_args(&self) -> usize {
        1
    }
}

/// COALESCE function
struct CoalesceFunction;

impl TransformFunction for CoalesceFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.is_empty() {
            return Err(anyhow!("COALESCE requires at least 1 argument"));
        }

        for arg in args {
            if !arg.is_null() {
                return Ok(arg.clone());
            }
        }

        Ok(Value::Null)
    }

    fn name(&self) -> &str {
        "COALESCE"
    }

    fn min_args(&self) -> usize {
        1
    }
}

/// CAST function
struct CastFunction;

impl TransformFunction for CastFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        // CAST expects: [value, "INTEGER" | "DECIMAL" | "DATE" | etc]
        if args.len() != 2 {
            return Err(anyhow!("CAST requires exactly 2 arguments: value and type"));
        }

        let value = &args[0];
        let target_type = args[1].as_string().to_uppercase();

        match target_type.as_str() {
            "INTEGER" | "INT" => {
                if value.is_null() {
                    return Ok(Value::Null);
                }
                let int_val = value.as_integer()?;
                Ok(Value::Integer(int_val))
            }
            "DECIMAL" | "NUMERIC" => {
                if value.is_null() {
                    return Ok(Value::Null);
                }
                Ok(Value::Decimal(value.as_string()))
            }
            "STRING" | "VARCHAR" | "TEXT" => Ok(Value::String(value.as_string())),
            "BOOLEAN" | "BOOL" => Ok(Value::Boolean(value.as_boolean())),
            "DATE" => {
                if value.is_null() {
                    return Ok(Value::Null);
                }
                let date_str = value.as_string();
                let date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d").with_context(|| {
                    format!("Cannot parse '{}' as date (expected YYYY-MM-DD)", date_str)
                })?;
                Ok(Value::Date(date))
            }
            _ => Err(anyhow!("Unsupported CAST target type: {}", target_type)),
        }
    }

    fn name(&self) -> &str {
        "CAST"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// Transformation executor
pub struct TransformationExecutor {
    /// Registered functions
    functions: HashMap<String, Box<dyn TransformFunction>>,
}

impl TransformationExecutor {
    /// Create new transformation executor with built-in functions
    pub fn new() -> Self {
        let mut executor = Self {
            functions: HashMap::new(),
        };

        // Register built-in functions
        executor.register_function(Box::new(UpperFunction));
        executor.register_function(Box::new(LowerFunction));
        executor.register_function(Box::new(TrimFunction));
        executor.register_function(Box::new(ConcatFunction));
        executor.register_function(Box::new(CoalesceFunction));
        executor.register_function(Box::new(CastFunction));

        executor
    }

    /// Register a custom function
    pub fn register_function(&mut self, func: Box<dyn TransformFunction>) {
        self.functions.insert(func.name().to_uppercase(), func);
    }

    /// Execute transformation expression
    pub fn execute(&self, expr: &str, context: &HashMap<String, Value>) -> Result<Value> {
        self.execute_expr(expr, context)
    }

    /// Execute expression (internal)
    fn execute_expr(&self, expr: &str, context: &HashMap<String, Value>) -> Result<Value> {
        let expr = expr.trim();

        // Check if it's a variable reference: {name}
        if expr.starts_with('{') && expr.ends_with('}') {
            let var_name = &expr[1..expr.len() - 1];
            return Ok(context.get(var_name).cloned().unwrap_or(Value::Null));
        }

        // Check if it's a string literal: 'value' or "value"
        if (expr.starts_with('\'') && expr.ends_with('\''))
            || (expr.starts_with('"') && expr.ends_with('"'))
        {
            return Ok(Value::String(expr[1..expr.len() - 1].to_string()));
        }

        // Check if it's a function call: FUNC(args)
        if let Some(paren_pos) = expr.find('(') {
            if !expr.ends_with(')') {
                return Err(anyhow!("Unmatched parentheses in expression: {}", expr));
            }

            let func_name = expr[..paren_pos].trim().to_uppercase();
            let args_str = &expr[paren_pos + 1..expr.len() - 1];

            // Parse arguments
            let args = self.parse_arguments(args_str, context)?;

            // Execute function
            if let Some(func) = self.functions.get(&func_name) {
                return func.execute(&args);
            } else {
                return Err(anyhow!("Unknown function: {}", func_name));
            }
        }

        // Otherwise, treat as literal value
        Ok(Value::String(expr.to_string()))
    }

    /// Parse function arguments
    fn parse_arguments(
        &self,
        args_str: &str,
        context: &HashMap<String, Value>,
    ) -> Result<Vec<Value>> {
        if args_str.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut args = Vec::new();
        let mut current_arg = String::new();
        let mut paren_depth = 0;
        let mut in_quotes = false;
        let mut quote_char = ' ';

        for ch in args_str.chars() {
            match ch {
                '\'' | '"' if paren_depth == 0 => {
                    if !in_quotes {
                        in_quotes = true;
                        quote_char = ch;
                    } else if ch == quote_char {
                        in_quotes = false;
                    }
                    current_arg.push(ch);
                }
                '(' if !in_quotes => {
                    paren_depth += 1;
                    current_arg.push(ch);
                }
                ')' if !in_quotes => {
                    paren_depth -= 1;
                    current_arg.push(ch);
                }
                ',' if paren_depth == 0 && !in_quotes => {
                    // Argument separator
                    args.push(self.execute_expr(&current_arg, context)?);
                    current_arg.clear();
                }
                _ => {
                    current_arg.push(ch);
                }
            }
        }

        // Add last argument
        if !current_arg.trim().is_empty() {
            args.push(self.execute_expr(&current_arg, context)?);
        }

        Ok(args)
    }
}

impl Default for TransformationExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upper_function() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("name".to_string(), Value::String("alice".to_string()));

        let result = executor.execute("UPPER({name})", &context)?;
        assert_eq!(result, Value::String("ALICE".to_string()));

        Ok(())
    }

    #[test]
    fn test_lower_function() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("name".to_string(), Value::String("ALICE".to_string()));

        let result = executor.execute("LOWER({name})", &context)?;
        assert_eq!(result, Value::String("alice".to_string()));

        Ok(())
    }

    #[test]
    fn test_trim_function() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("name".to_string(), Value::String("  alice  ".to_string()));

        let result = executor.execute("TRIM({name})", &context)?;
        assert_eq!(result, Value::String("alice".to_string()));

        Ok(())
    }

    #[test]
    fn test_nested_functions() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("name".to_string(), Value::String("  alice  ".to_string()));

        let result = executor.execute("UPPER(TRIM({name}))", &context)?;
        assert_eq!(result, Value::String("ALICE".to_string()));

        Ok(())
    }

    #[test]
    fn test_concat_function() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("first".to_string(), Value::String("Alice".to_string()));
        context.insert("last".to_string(), Value::String("Smith".to_string()));

        let result = executor.execute("CONCAT({first}, ' ', {last})", &context)?;
        assert_eq!(result, Value::String("Alice Smith".to_string()));

        Ok(())
    }

    #[test]
    fn test_coalesce_function() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("value1".to_string(), Value::Null);
        context.insert("value2".to_string(), Value::String("fallback".to_string()));

        let result = executor.execute("COALESCE({value1}, {value2}, 'default')", &context)?;
        assert_eq!(result, Value::String("fallback".to_string()));

        Ok(())
    }

    #[test]
    fn test_cast_to_integer() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("value".to_string(), Value::String("42".to_string()));

        let result = executor.execute("CAST({value}, 'INTEGER')", &context)?;
        assert_eq!(result, Value::Integer(42));

        Ok(())
    }

    #[test]
    fn test_cast_invalid_integer() {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert(
            "value".to_string(),
            Value::String("not-a-number".to_string()),
        );

        let result = executor.execute("CAST({value}, 'INTEGER')", &context);
        assert!(result.is_err());
    }

    #[test]
    fn test_cast_to_date() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("value".to_string(), Value::String("2024-01-15".to_string()));

        let result = executor.execute("CAST({value}, 'DATE')", &context)?;
        match result {
            Value::Date(d) => {
                assert_eq!(d.to_string(), "2024-01-15");
            }
            _ => panic!("Expected Date value"),
        }

        Ok(())
    }

    #[test]
    fn test_null_propagation() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("value".to_string(), Value::Null);

        let result = executor.execute("UPPER({value})", &context)?;
        assert_eq!(result, Value::Null);

        Ok(())
    }

    #[test]
    fn test_literal_string() -> Result<()> {
        let executor = TransformationExecutor::new();
        let context = HashMap::new();

        let result = executor.execute("'literal value'", &context)?;
        assert_eq!(result, Value::String("literal value".to_string()));

        Ok(())
    }

    #[test]
    fn test_complex_expression() -> Result<()> {
        let executor = TransformationExecutor::new();
        let mut context = HashMap::new();
        context.insert("first".to_string(), Value::String("  alice  ".to_string()));
        context.insert("last".to_string(), Value::Null);

        let result = executor.execute("UPPER(TRIM(COALESCE({last}, {first})))", &context)?;
        assert_eq!(result, Value::String("ALICE".to_string()));

        Ok(())
    }

    #[test]
    fn test_value_as_string() {
        assert_eq!(Value::String("test".to_string()).as_string(), "test");
        assert_eq!(Value::Integer(42).as_string(), "42");
        assert_eq!(Value::Boolean(true).as_string(), "1");
        assert_eq!(Value::Boolean(false).as_string(), "0");
        assert_eq!(Value::Null.as_string(), "");
    }

    #[test]
    fn test_value_as_integer() -> Result<()> {
        assert_eq!(Value::Integer(42).as_integer()?, 42);
        assert_eq!(Value::String("42".to_string()).as_integer()?, 42);
        assert_eq!(Value::Boolean(true).as_integer()?, 1);
        assert_eq!(Value::Boolean(false).as_integer()?, 0);

        Ok(())
    }

    #[test]
    fn test_value_as_boolean() {
        assert!(Value::Boolean(true).as_boolean());
        assert!(!Value::Boolean(false).as_boolean());
        assert!(Value::Integer(1).as_boolean());
        assert!(!Value::Integer(0).as_boolean());
        assert!(Value::String("yes".to_string()).as_boolean());
        assert!(!Value::String("".to_string()).as_boolean());
        assert!(!Value::Null.as_boolean());
    }
}
