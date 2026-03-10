//! Abstract Syntax Tree for transformation expressions

use serde::{Deserialize, Serialize};
use std::fmt;

/// Expression AST node
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    /// Literal value
    Literal(Literal),

    /// Variable reference: {field_name}
    Variable(String),

    /// Function call: FUNC(args...)
    FunctionCall { name: String, args: Vec<Expression> },

    /// Binary operation: expr1 + expr2
    BinaryOp {
        left: Box<Expression>,
        op: BinaryOp,
        right: Box<Expression>,
    },

    /// Unary operation: -expr
    UnaryOp { op: UnaryOp, expr: Box<Expression> },

    /// Conditional: CASE WHEN condition THEN expr1 ELSE expr2 END
    Case {
        conditions: Vec<(Box<Expression>, Box<Expression>)>,
        else_expr: Option<Box<Expression>>,
    },

    /// Type cast: CAST(expr AS type)
    Cast {
        expr: Box<Expression>,
        target_type: DataType,
    },
}

/// Literal values
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Literal {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Null,
}

/// Binary operators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,

    // String
    Concat,

    // Comparison
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,

    // Logical
    And,
    Or,

    // Pattern matching
    Like,
    NotLike,
}

/// Unary operators
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    // Arithmetic
    Negate,

    // Logical
    Not,

    // NULL check
    IsNull,
    IsNotNull,
}

/// Data types for type checking and casting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DataType {
    String,
    Integer,
    Float,
    Boolean,
    Date,
    Timestamp,
    Decimal { precision: u8, scale: u8 },
    Unknown,
}

/// Built-in function definitions
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    // String functions
    Upper,
    Lower,
    Trim,
    LTrim,
    RTrim,
    Length,
    Substring,
    Replace,
    Concat,

    // Numeric functions
    Abs,
    Round,
    Floor,
    Ceil,

    // Date functions
    CurrentDate,
    DateAdd,
    DateDiff,
    DateFormat,

    // Null handling
    Coalesce,
    NullIf,

    // Type conversion
    Cast,

    // Custom function
    Custom(String),
}

impl Expression {
    /// Check if expression is a constant (can be evaluated at compile time)
    pub fn is_constant(&self) -> bool {
        match self {
            Expression::Literal(_) => true,
            Expression::FunctionCall { name, args } => {
                // Some functions with constant args are constant
                matches!(name.as_str(), "CURRENT_DATE" | "PI" | "E")
                    || (args.iter().all(|arg| arg.is_constant()))
            }
            Expression::BinaryOp { left, right, .. } => left.is_constant() && right.is_constant(),
            Expression::UnaryOp { expr, .. } => expr.is_constant(),
            Expression::Cast { expr, .. } => expr.is_constant(),
            _ => false,
        }
    }

    /// Get the inferred data type of the expression
    pub fn infer_type(&self) -> DataType {
        match self {
            Expression::Literal(lit) => match lit {
                Literal::String(_) => DataType::String,
                Literal::Integer(_) => DataType::Integer,
                Literal::Float(_) => DataType::Float,
                Literal::Boolean(_) => DataType::Boolean,
                Literal::Null => DataType::Unknown,
            },
            Expression::Variable(_) => DataType::Unknown,
            Expression::FunctionCall { name, .. } => match name.to_uppercase().as_str() {
                "UPPER" | "LOWER" | "TRIM" | "LTRIM" | "RTRIM" | "REPLACE" | "CONCAT" => {
                    DataType::String
                }
                "LENGTH" | "ABS" | "ROUND" | "FLOOR" | "CEIL" => DataType::Integer,
                "CURRENT_DATE" => DataType::Date,
                _ => DataType::Unknown,
            },
            Expression::BinaryOp { op, left, .. } => {
                match op {
                    BinaryOp::Concat => DataType::String,
                    BinaryOp::Add
                    | BinaryOp::Subtract
                    | BinaryOp::Multiply
                    | BinaryOp::Divide
                    | BinaryOp::Modulo => {
                        // Type promotion rules
                        match left.infer_type() {
                            DataType::Float | DataType::Decimal { .. } => DataType::Float,
                            _ => DataType::Integer,
                        }
                    }
                    BinaryOp::Equal
                    | BinaryOp::NotEqual
                    | BinaryOp::LessThan
                    | BinaryOp::LessThanOrEqual
                    | BinaryOp::GreaterThan
                    | BinaryOp::GreaterThanOrEqual
                    | BinaryOp::And
                    | BinaryOp::Or
                    | BinaryOp::Like
                    | BinaryOp::NotLike => DataType::Boolean,
                }
            }
            Expression::UnaryOp { op, expr } => match op {
                UnaryOp::Negate => expr.infer_type(),
                UnaryOp::Not | UnaryOp::IsNull | UnaryOp::IsNotNull => DataType::Boolean,
            },
            Expression::Case {
                conditions,
                else_expr,
            } => {
                // Return type of first THEN clause or ELSE clause
                if let Some((_, then_expr)) = conditions.first() {
                    then_expr.infer_type()
                } else if let Some(else_expr) = else_expr {
                    else_expr.infer_type()
                } else {
                    DataType::Unknown
                }
            }
            Expression::Cast { target_type, .. } => target_type.clone(),
        }
    }

    /// Check if expression contains any variables
    pub fn has_variables(&self) -> bool {
        match self {
            Expression::Variable(_) => true,
            Expression::FunctionCall { args, .. } => args.iter().any(|arg| arg.has_variables()),
            Expression::BinaryOp { left, right, .. } => {
                left.has_variables() || right.has_variables()
            }
            Expression::UnaryOp { expr, .. } => expr.has_variables(),
            Expression::Case {
                conditions,
                else_expr,
            } => {
                conditions
                    .iter()
                    .any(|(cond, then_expr)| cond.has_variables() || then_expr.has_variables())
                    || else_expr
                        .as_ref()
                        .map(|e| e.has_variables())
                        .unwrap_or(false)
            }
            Expression::Cast { expr, .. } => expr.has_variables(),
            _ => false,
        }
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expression::Literal(lit) => match lit {
                Literal::String(s) => write!(f, "'{}'", s),
                Literal::Integer(i) => write!(f, "{}", i),
                Literal::Float(fl) => write!(f, "{}", fl),
                Literal::Boolean(b) => write!(f, "{}", b),
                Literal::Null => write!(f, "NULL"),
            },
            Expression::Variable(name) => write!(f, "{{{}}}", name),
            Expression::FunctionCall { name, args } => {
                write!(f, "{}(", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ")")
            }
            Expression::BinaryOp { left, op, right } => {
                write!(f, "({} {} {})", left, op, right)
            }
            Expression::UnaryOp { op, expr } => write!(f, "{}{}", op, expr),
            Expression::Case {
                conditions,
                else_expr,
            } => {
                write!(f, "CASE")?;
                for (cond, then_expr) in conditions {
                    write!(f, " WHEN {} THEN {}", cond, then_expr)?;
                }
                if let Some(else_expr) = else_expr {
                    write!(f, " ELSE {}", else_expr)?;
                }
                write!(f, " END")
            }
            Expression::Cast { expr, target_type } => {
                write!(f, "CAST({} AS {:?})", expr, target_type)
            }
        }
    }
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op_str = match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Modulo => "%",
            BinaryOp::Concat => "||",
            BinaryOp::Equal => "=",
            BinaryOp::NotEqual => "!=",
            BinaryOp::LessThan => "<",
            BinaryOp::LessThanOrEqual => "<=",
            BinaryOp::GreaterThan => ">",
            BinaryOp::GreaterThanOrEqual => ">=",
            BinaryOp::And => "AND",
            BinaryOp::Or => "OR",
            BinaryOp::Like => "LIKE",
            BinaryOp::NotLike => "NOT LIKE",
        };
        write!(f, "{}", op_str)
    }
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let op_str = match self {
            UnaryOp::Negate => "-",
            UnaryOp::Not => "NOT ",
            UnaryOp::IsNull => "IS NULL",
            UnaryOp::IsNotNull => "IS NOT NULL",
        };
        write!(f, "{}", op_str)
    }
}
