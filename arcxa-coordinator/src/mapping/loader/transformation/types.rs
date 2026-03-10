//! Type system for transformation engine

use anyhow::{anyhow, Result};
use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

use super::ast::{DataType, Expression};

/// Runtime value types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// String value (using Cow for zero-copy when possible)
    String(Cow<'static, str>),

    /// Integer value
    Integer(i64),

    /// Float value
    Float(f64),

    /// Decimal value with precision
    Decimal(Decimal),

    /// Boolean value
    Boolean(bool),

    /// Date value
    Date(NaiveDate),

    /// Timestamp value
    Timestamp(NaiveDateTime),

    /// Null value
    Null,

    /// Array value
    Array(Vec<Value>),
}

impl Value {
    /// Create a string value from a static string (zero-copy)
    pub fn string_static(s: &'static str) -> Self {
        Value::String(Cow::Borrowed(s))
    }

    /// Create a string value from an owned String
    pub fn string_owned(s: String) -> Self {
        Value::String(Cow::Owned(s))
    }

    /// Check if value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Get the data type of the value
    pub fn data_type(&self) -> DataType {
        match self {
            Value::String(_) => DataType::String,
            Value::Integer(_) => DataType::Integer,
            Value::Float(_) => DataType::Float,
            Value::Decimal(_) => DataType::Decimal {
                precision: 38,
                scale: 10,
            },
            Value::Boolean(_) => DataType::Boolean,
            Value::Date(_) => DataType::Date,
            Value::Timestamp(_) => DataType::Timestamp,
            Value::Null => DataType::Unknown,
            Value::Array(_) => DataType::Unknown,
        }
    }

    /// Convert value to string representation
    pub fn as_string(&self) -> Cow<'_, str> {
        match self {
            Value::String(s) => s.clone(),
            Value::Integer(i) => Cow::Owned(i.to_string()),
            Value::Float(f) => Cow::Owned(f.to_string()),
            Value::Decimal(d) => Cow::Owned(d.to_string()),
            Value::Boolean(b) => Cow::Borrowed(if *b { "true" } else { "false" }),
            Value::Date(d) => Cow::Owned(d.format("%Y-%m-%d").to_string()),
            Value::Timestamp(t) => Cow::Owned(t.format("%Y-%m-%d %H:%M:%S").to_string()),
            Value::Null => Cow::Borrowed(""),
            Value::Array(arr) => {
                let strings: Vec<String> = arr.iter().map(|v| v.as_string().into_owned()).collect();
                Cow::Owned(strings.join(","))
            }
        }
    }

    /// Try to convert value to integer
    pub fn as_integer(&self) -> Result<i64> {
        match self {
            Value::Integer(i) => Ok(*i),
            Value::Float(f) => Ok(*f as i64),
            Value::Decimal(d) => Ok(d.to_i64().unwrap_or(0)),
            Value::String(s) => s
                .trim()
                .parse()
                .map_err(|e| anyhow!("Cannot convert '{}' to integer: {}", s, e)),
            Value::Boolean(b) => Ok(if *b { 1 } else { 0 }),
            Value::Null => Err(anyhow!("Cannot convert NULL to integer")),
            _ => Err(anyhow!("Cannot convert {:?} to integer", self)),
        }
    }

    /// Try to convert value to float
    pub fn as_float(&self) -> Result<f64> {
        match self {
            Value::Float(f) => Ok(*f),
            Value::Integer(i) => Ok(*i as f64),
            Value::Decimal(d) => Ok(d.to_f64().unwrap_or(0.0)),
            Value::String(s) => s
                .trim()
                .parse()
                .map_err(|e| anyhow!("Cannot convert '{}' to float: {}", s, e)),
            Value::Boolean(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Null => Err(anyhow!("Cannot convert NULL to float")),
            _ => Err(anyhow!("Cannot convert {:?} to float", self)),
        }
    }

    /// Try to convert value to boolean
    pub fn as_boolean(&self) -> bool {
        match self {
            Value::Boolean(b) => *b,
            Value::Integer(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty() && s != "0" && s.to_lowercase() != "false",
            Value::Null => false,
            _ => true,
        }
    }

    /// Try to convert value to date
    pub fn as_date(&self) -> Result<NaiveDate> {
        match self {
            Value::Date(d) => Ok(*d),
            Value::Timestamp(t) => Ok(t.date()),
            Value::String(s) => NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                .map_err(|e| anyhow!("Cannot parse '{}' as date: {}", s, e)),
            _ => Err(anyhow!("Cannot convert {:?} to date", self)),
        }
    }

    /// Cast value to target type
    pub fn cast(&self, target_type: &DataType) -> Result<Value> {
        match target_type {
            DataType::String => Ok(Value::String(Cow::Owned(self.as_string().into_owned()))),
            DataType::Integer => Ok(Value::Integer(self.as_integer()?)),
            DataType::Float => Ok(Value::Float(self.as_float()?)),
            DataType::Boolean => Ok(Value::Boolean(self.as_boolean())),
            DataType::Date => Ok(Value::Date(self.as_date()?)),
            DataType::Decimal { .. } => match self {
                Value::Decimal(d) => Ok(Value::Decimal(*d)),
                Value::String(s) => {
                    Ok(Value::Decimal(s.parse().map_err(|e| {
                        anyhow!("Cannot parse '{}' as decimal: {}", s, e)
                    })?))
                }
                Value::Integer(i) => Ok(Value::Decimal(Decimal::from(*i))),
                Value::Float(f) => Ok(Value::Decimal(
                    Decimal::from_f64_retain(*f).unwrap_or(Decimal::ZERO),
                )),
                _ => Err(anyhow!("Cannot convert {:?} to decimal", self)),
            },
            DataType::Timestamp => match self {
                Value::Timestamp(t) => Ok(Value::Timestamp(*t)),
                Value::String(s) => Ok(Value::Timestamp(
                    NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
                        .map_err(|e| anyhow!("Cannot parse '{}' as timestamp: {}", s, e))?,
                )),
                _ => Err(anyhow!("Cannot convert {:?} to timestamp", self)),
            },
            DataType::Unknown => Ok(self.clone()),
        }
    }

    /// Coerce two values to a common type for comparison
    pub fn coerce_types(left: &Value, right: &Value) -> (Value, Value) {
        match (left, right) {
            // If either is NULL, no coercion needed
            (Value::Null, _) | (_, Value::Null) => (left.clone(), right.clone()),

            // Same types, no coercion needed
            (Value::String(_), Value::String(_))
            | (Value::Integer(_), Value::Integer(_))
            | (Value::Float(_), Value::Float(_))
            | (Value::Boolean(_), Value::Boolean(_))
            | (Value::Date(_), Value::Date(_)) => (left.clone(), right.clone()),

            // Numeric coercion
            (Value::Integer(i), Value::Float(_)) => (Value::Float(*i as f64), right.clone()),
            (Value::Float(_), Value::Integer(i)) => (left.clone(), Value::Float(*i as f64)),

            // String coercion (convert both to string for comparison)
            _ => (
                Value::String(Cow::Owned(left.as_string().into_owned())),
                Value::String(Cow::Owned(right.as_string().into_owned())),
            ),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Integer(i) => write!(f, "{}", i),
            Value::Float(fl) => write!(f, "{}", fl),
            Value::Decimal(d) => write!(f, "{}", d),
            Value::Boolean(b) => write!(f, "{}", b),
            Value::Date(d) => write!(f, "{}", d),
            Value::Timestamp(t) => write!(f, "{}", t),
            Value::Null => write!(f, "NULL"),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", v)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Type checker for expressions
pub struct TypeChecker {
    /// Whether to perform strict type checking
    strict_mode: bool,
}

impl TypeChecker {
    /// Create a new type checker
    pub fn new() -> Self {
        Self { strict_mode: false }
    }

    /// Create a type checker with strict mode
    pub fn strict() -> Self {
        Self { strict_mode: true }
    }

    /// Check expression for type correctness
    pub fn check(&self, expr: &Expression) -> Result<DataType> {
        match expr {
            Expression::Literal(lit) => Ok(match lit {
                super::ast::Literal::String(_) => DataType::String,
                super::ast::Literal::Integer(_) => DataType::Integer,
                super::ast::Literal::Float(_) => DataType::Float,
                super::ast::Literal::Boolean(_) => DataType::Boolean,
                super::ast::Literal::Null => DataType::Unknown,
            }),

            Expression::Variable(_) => {
                // Variable types are unknown at compile time
                Ok(DataType::Unknown)
            }

            Expression::FunctionCall { name, args } => self.check_function_types(name, args),

            Expression::BinaryOp { left, op, right } => {
                let left_type = self.check(left)?;
                let right_type = self.check(right)?;
                self.check_binary_op_types(&left_type, op, &right_type)
            }

            Expression::UnaryOp { op, expr } => {
                let expr_type = self.check(expr)?;
                self.check_unary_op_type(op, &expr_type)
            }

            Expression::Cast { target_type, .. } => Ok(target_type.clone()),

            Expression::Case {
                conditions,
                else_expr,
            } => {
                // Check all WHEN conditions are boolean
                for (cond, _) in conditions {
                    let cond_type = self.check(cond)?;
                    if self.strict_mode
                        && cond_type != DataType::Boolean
                        && cond_type != DataType::Unknown
                    {
                        return Err(anyhow!(
                            "CASE condition must be boolean, got {:?}",
                            cond_type
                        ));
                    }
                }

                // Return type of first THEN clause or ELSE clause
                if let Some((_, then_expr)) = conditions.first() {
                    self.check(then_expr)
                } else if let Some(else_expr) = else_expr {
                    self.check(else_expr)
                } else {
                    Ok(DataType::Unknown)
                }
            }
        }
    }

    /// Check function argument types
    fn check_function_types(&self, name: &str, args: &[Expression]) -> Result<DataType> {
        match name.to_uppercase().as_str() {
            "UPPER" | "LOWER" | "TRIM" | "LTRIM" | "RTRIM" => {
                if args.len() != 1 {
                    return Err(anyhow!("{} requires exactly 1 argument", name));
                }
                Ok(DataType::String)
            }

            "CONCAT" => {
                if args.is_empty() {
                    return Err(anyhow!("CONCAT requires at least 1 argument"));
                }
                Ok(DataType::String)
            }

            "LENGTH" => {
                if args.len() != 1 {
                    return Err(anyhow!("LENGTH requires exactly 1 argument"));
                }
                Ok(DataType::Integer)
            }

            "SUBSTRING" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err(anyhow!("SUBSTRING requires 2 or 3 arguments"));
                }
                Ok(DataType::String)
            }

            "COALESCE" => {
                if args.is_empty() {
                    return Err(anyhow!("COALESCE requires at least 1 argument"));
                }
                // Return type of first non-null argument
                for arg in args {
                    let arg_type = self.check(arg)?;
                    if arg_type != DataType::Unknown {
                        return Ok(arg_type);
                    }
                }
                Ok(DataType::Unknown)
            }

            _ => Ok(DataType::Unknown),
        }
    }

    /// Check binary operator type compatibility
    fn check_binary_op_types(
        &self,
        left: &DataType,
        op: &super::ast::BinaryOp,
        right: &DataType,
    ) -> Result<DataType> {
        use super::ast::BinaryOp;

        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Modulo => {
                // Arithmetic operations
                if self.strict_mode {
                    match (left, right) {
                        (DataType::Integer, DataType::Integer) => Ok(DataType::Integer),
                        (DataType::Float, _) | (_, DataType::Float) => Ok(DataType::Float),
                        (DataType::Unknown, _) | (_, DataType::Unknown) => Ok(DataType::Unknown),
                        _ => Err(anyhow!(
                            "Incompatible types for arithmetic: {:?} and {:?}",
                            left,
                            right
                        )),
                    }
                } else {
                    Ok(DataType::Float)
                }
            }

            BinaryOp::Concat => Ok(DataType::String),

            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::LessThan
            | BinaryOp::LessThanOrEqual
            | BinaryOp::GreaterThan
            | BinaryOp::GreaterThanOrEqual => Ok(DataType::Boolean),

            BinaryOp::And | BinaryOp::Or => {
                if self.strict_mode && left != &DataType::Boolean && left != &DataType::Unknown {
                    return Err(anyhow!("Logical operators require boolean operands"));
                }
                Ok(DataType::Boolean)
            }

            BinaryOp::Like | BinaryOp::NotLike => {
                if self.strict_mode && left != &DataType::String && left != &DataType::Unknown {
                    return Err(anyhow!("LIKE operator requires string operands"));
                }
                Ok(DataType::Boolean)
            }
        }
    }

    /// Check unary operator type compatibility
    fn check_unary_op_type(
        &self,
        op: &super::ast::UnaryOp,
        expr_type: &DataType,
    ) -> Result<DataType> {
        use super::ast::UnaryOp;

        match op {
            UnaryOp::Negate => {
                if self.strict_mode
                    && !matches!(
                        expr_type,
                        DataType::Integer | DataType::Float | DataType::Unknown
                    )
                {
                    return Err(anyhow!("Negation requires numeric operand"));
                }
                Ok(expr_type.clone())
            }

            UnaryOp::Not => {
                if self.strict_mode
                    && expr_type != &DataType::Boolean
                    && expr_type != &DataType::Unknown
                {
                    return Err(anyhow!("NOT requires boolean operand"));
                }
                Ok(DataType::Boolean)
            }

            UnaryOp::IsNull | UnaryOp::IsNotNull => Ok(DataType::Boolean),
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}
