//! High-performance transformation executor
//!
//! Executes compiled transformation plans with optimizations for throughput.

use anyhow::{anyhow, Result};
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use super::ast::{BinaryOp, Expression, Literal, UnaryOp};
use super::functions::FunctionRegistry;
use super::types::Value;

/// Compiled execution plan for a transformation
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    /// Root expression
    root: Expression,

    /// Pre-computed constant expressions
    constants: HashMap<String, Value>,

    /// Required input fields
    required_fields: Vec<String>,

    /// Whether the plan can be executed in parallel
    is_parallel_safe: bool,
}

impl ExecutionPlan {
    /// Build execution plan from AST
    pub fn from_ast(expr: Expression, functions: &Arc<FunctionRegistry>) -> Result<Self> {
        let mut plan = Self {
            root: expr.clone(),
            constants: HashMap::new(),
            required_fields: Vec::new(),
            is_parallel_safe: true,
        };

        // Extract required fields
        plan.extract_required_fields(&expr);

        // Pre-compute constants
        plan.precompute_constants(&expr, functions)?;

        // Check if parallel safe
        plan.is_parallel_safe = plan.check_parallel_safety(&expr);

        Ok(plan)
    }

    /// Extract list of required input fields
    fn extract_required_fields(&mut self, expr: &Expression) {
        match expr {
            Expression::Variable(name) => {
                if !self.required_fields.contains(name) {
                    self.required_fields.push(name.clone());
                }
            }
            Expression::FunctionCall { args, .. } => {
                for arg in args {
                    self.extract_required_fields(arg);
                }
            }
            Expression::BinaryOp { left, right, .. } => {
                self.extract_required_fields(left);
                self.extract_required_fields(right);
            }
            Expression::UnaryOp { expr, .. } => {
                self.extract_required_fields(expr);
            }
            Expression::Case {
                conditions,
                else_expr,
            } => {
                for (cond, then_expr) in conditions {
                    self.extract_required_fields(cond);
                    self.extract_required_fields(then_expr);
                }
                if let Some(else_expr) = else_expr {
                    self.extract_required_fields(else_expr);
                }
            }
            Expression::Cast { expr, .. } => {
                self.extract_required_fields(expr);
            }
            _ => {}
        }
    }

    /// Pre-compute constant expressions
    fn precompute_constants(
        &mut self,
        expr: &Expression,
        functions: &Arc<FunctionRegistry>,
    ) -> Result<()> {
        if expr.is_constant() && !expr.has_variables() {
            let executor = SimpleExecutor::new(functions.clone());
            match executor.eval_expr(expr, &HashMap::new()) {
                Ok(value) => {
                    let key = format!("{:?}", expr);
                    self.constants.insert(key, value);
                }
                Err(_) => {
                    // Ignore errors for constants that can't be pre-computed
                }
            }
        }

        // Recursively process sub-expressions
        match expr {
            Expression::FunctionCall { args, .. } => {
                for arg in args {
                    self.precompute_constants(arg, functions)?;
                }
            }
            Expression::BinaryOp { left, right, .. } => {
                self.precompute_constants(left, functions)?;
                self.precompute_constants(right, functions)?;
            }
            Expression::UnaryOp { expr, .. } => {
                self.precompute_constants(expr, functions)?;
            }
            Expression::Case {
                conditions,
                else_expr,
            } => {
                for (cond, then_expr) in conditions {
                    self.precompute_constants(cond, functions)?;
                    self.precompute_constants(then_expr, functions)?;
                }
                if let Some(else_expr) = else_expr {
                    self.precompute_constants(else_expr, functions)?;
                }
            }
            Expression::Cast { expr, .. } => {
                self.precompute_constants(expr, functions)?;
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if the plan can be executed in parallel
    fn check_parallel_safety(&self, expr: &Expression) -> bool {
        // For now, assume all expressions are parallel safe
        // In future, we might check for stateful functions
        true
    }
}

/// High-performance transformation executor
pub struct TransformationExecutor {
    /// Function registry
    functions: Arc<FunctionRegistry>,

    /// Number of parallel workers
    num_workers: usize,
}

impl TransformationExecutor {
    /// Create a new executor
    pub fn new(functions: Arc<FunctionRegistry>) -> Self {
        Self {
            functions,
            num_workers: num_cpus::get(),
        }
    }

    /// Execute transformation on a single row
    pub fn execute_single(
        &self,
        plan: &ExecutionPlan,
        row: &HashMap<String, String>,
    ) -> Result<Value> {
        // Convert string values to Value type
        let mut context = HashMap::new();
        for (key, value) in row {
            context.insert(key.clone(), Value::string_owned(value.clone()));
        }

        let executor = SimpleExecutor::new(self.functions.clone());
        executor.eval_expr(&plan.root, &context)
    }

    /// Execute transformation on a batch of rows in parallel
    pub async fn execute_batch(
        &self,
        plan: &ExecutionPlan,
        batch: Vec<HashMap<String, String>>,
    ) -> Result<Vec<Result<Value>>> {
        if plan.is_parallel_safe && batch.len() > 100 {
            // Use parallel execution for large batches
            let plan = plan.clone();
            let functions = self.functions.clone();

            let results: Vec<Result<Value>> = tokio::task::spawn_blocking(move || {
                batch
                    .into_par_iter()
                    .map(|row| {
                        let mut context = HashMap::new();
                        for (key, value) in row {
                            context.insert(key, Value::string_owned(value));
                        }
                        let executor = SimpleExecutor::new(functions.clone());
                        executor.eval_expr(&plan.root, &context)
                    })
                    .collect()
            })
            .await?;

            Ok(results)
        } else {
            // Sequential execution for small batches
            Ok(batch
                .into_iter()
                .map(|row| self.execute_single(plan, &row))
                .collect())
        }
    }
}

/// Simple expression evaluator
struct SimpleExecutor {
    functions: Arc<FunctionRegistry>,
}

impl SimpleExecutor {
    fn new(functions: Arc<FunctionRegistry>) -> Self {
        Self { functions }
    }

    /// Evaluate an expression
    fn eval_expr(&self, expr: &Expression, context: &HashMap<String, Value>) -> Result<Value> {
        match expr {
            Expression::Literal(lit) => Ok(self.eval_literal(lit)),

            Expression::Variable(name) => context
                .get(name)
                .cloned()
                .ok_or_else(|| anyhow!("Variable '{}' not found in context", name)),

            Expression::FunctionCall { name, args } => {
                let arg_values: Result<Vec<_>> = args
                    .iter()
                    .map(|arg| self.eval_expr(arg, context))
                    .collect();
                self.functions.execute(name, &arg_values?)
            }

            Expression::BinaryOp { left, op, right } => {
                let left_val = self.eval_expr(left, context)?;
                let right_val = self.eval_expr(right, context)?;
                self.eval_binary_op(&left_val, op, &right_val)
            }

            Expression::UnaryOp { op, expr } => {
                let val = self.eval_expr(expr, context)?;
                self.eval_unary_op(op, &val)
            }

            Expression::Case {
                conditions,
                else_expr,
            } => {
                for (cond, then_expr) in conditions {
                    let cond_val = self.eval_expr(cond, context)?;
                    if cond_val.as_boolean() {
                        return self.eval_expr(then_expr, context);
                    }
                }
                if let Some(else_expr) = else_expr {
                    self.eval_expr(else_expr, context)
                } else {
                    Ok(Value::Null)
                }
            }

            Expression::Cast { expr, target_type } => {
                let val = self.eval_expr(expr, context)?;
                val.cast(target_type)
            }
        }
    }

    /// Evaluate a literal value
    fn eval_literal(&self, lit: &Literal) -> Value {
        match lit {
            Literal::String(s) => Value::string_owned(s.clone()),
            Literal::Integer(i) => Value::Integer(*i),
            Literal::Float(f) => Value::Float(*f),
            Literal::Boolean(b) => Value::Boolean(*b),
            Literal::Null => Value::Null,
        }
    }

    /// Evaluate binary operation
    fn eval_binary_op(&self, left: &Value, op: &BinaryOp, right: &Value) -> Result<Value> {
        // Handle NULL propagation
        if matches!(op, BinaryOp::Equal | BinaryOp::NotEqual) {
            // NULL comparisons have special rules
        } else if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        match op {
            // Arithmetic
            BinaryOp::Add => match (left, right) {
                (Value::Integer(l), Value::Integer(r)) => Ok(Value::Integer(l + r)),
                (Value::Float(l), Value::Float(r)) => Ok(Value::Float(l + r)),
                _ => {
                    let l = left.as_float()?;
                    let r = right.as_float()?;
                    Ok(Value::Float(l + r))
                }
            },

            BinaryOp::Subtract => match (left, right) {
                (Value::Integer(l), Value::Integer(r)) => Ok(Value::Integer(l - r)),
                (Value::Float(l), Value::Float(r)) => Ok(Value::Float(l - r)),
                _ => {
                    let l = left.as_float()?;
                    let r = right.as_float()?;
                    Ok(Value::Float(l - r))
                }
            },

            BinaryOp::Multiply => match (left, right) {
                (Value::Integer(l), Value::Integer(r)) => Ok(Value::Integer(l * r)),
                (Value::Float(l), Value::Float(r)) => Ok(Value::Float(l * r)),
                _ => {
                    let l = left.as_float()?;
                    let r = right.as_float()?;
                    Ok(Value::Float(l * r))
                }
            },

            BinaryOp::Divide => {
                let l = left.as_float()?;
                let r = right.as_float()?;
                if r == 0.0 {
                    return Err(anyhow!("Division by zero"));
                }
                Ok(Value::Float(l / r))
            }

            BinaryOp::Modulo => match (left, right) {
                (Value::Integer(l), Value::Integer(r)) => {
                    if *r == 0 {
                        return Err(anyhow!("Modulo by zero"));
                    }
                    Ok(Value::Integer(l % r))
                }
                _ => Err(anyhow!("Modulo requires integer operands")),
            },

            // String
            BinaryOp::Concat => {
                let l = left.as_string();
                let r = right.as_string();
                Ok(Value::string_owned(format!("{}{}", l, r)))
            }

            // Comparison
            BinaryOp::Equal => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(l == r))
            }

            BinaryOp::NotEqual => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(l != r))
            }

            BinaryOp::LessThan => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(self.compare_values(&l, &r)? < 0))
            }

            BinaryOp::LessThanOrEqual => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(self.compare_values(&l, &r)? <= 0))
            }

            BinaryOp::GreaterThan => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(self.compare_values(&l, &r)? > 0))
            }

            BinaryOp::GreaterThanOrEqual => {
                let (l, r) = Value::coerce_types(left, right);
                Ok(Value::Boolean(self.compare_values(&l, &r)? >= 0))
            }

            // Logical
            BinaryOp::And => Ok(Value::Boolean(left.as_boolean() && right.as_boolean())),

            BinaryOp::Or => Ok(Value::Boolean(left.as_boolean() || right.as_boolean())),

            // Pattern matching
            BinaryOp::Like => {
                let text = left.as_string();
                let pattern = right.as_string();
                Ok(Value::Boolean(self.match_like_pattern(&text, &pattern)))
            }

            BinaryOp::NotLike => {
                let text = left.as_string();
                let pattern = right.as_string();
                Ok(Value::Boolean(!self.match_like_pattern(&text, &pattern)))
            }
        }
    }

    /// Evaluate unary operation
    fn eval_unary_op(&self, op: &UnaryOp, val: &Value) -> Result<Value> {
        match op {
            UnaryOp::Negate => match val {
                Value::Integer(i) => Ok(Value::Integer(-i)),
                Value::Float(f) => Ok(Value::Float(-f)),
                Value::Null => Ok(Value::Null),
                _ => Err(anyhow!("Cannot negate {:?}", val)),
            },

            UnaryOp::Not => Ok(Value::Boolean(!val.as_boolean())),

            UnaryOp::IsNull => Ok(Value::Boolean(val.is_null())),

            UnaryOp::IsNotNull => Ok(Value::Boolean(!val.is_null())),
        }
    }

    /// Compare two values
    fn compare_values(&self, left: &Value, right: &Value) -> Result<i32> {
        match (left, right) {
            (Value::Integer(l), Value::Integer(r)) => Ok(l.cmp(r) as i32),
            (Value::Float(l), Value::Float(r)) => {
                if l < r {
                    Ok(-1)
                } else if l > r {
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            (Value::String(l), Value::String(r)) => Ok(l.cmp(r) as i32),
            (Value::Boolean(l), Value::Boolean(r)) => Ok(l.cmp(r) as i32),
            _ => Err(anyhow!("Cannot compare {:?} and {:?}", left, right)),
        }
    }

    /// Match SQL LIKE pattern (simplified)
    fn match_like_pattern(&self, text: &str, pattern: &str) -> bool {
        // Simple LIKE implementation (% = wildcard)
        // TODO: Implement full SQL LIKE with escape characters
        if pattern.contains('%') {
            let parts: Vec<&str> = pattern.split('%').collect();
            if parts.is_empty() {
                return true;
            }

            let mut pos = 0;
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }

                if i == 0 && !pattern.starts_with('%') {
                    // Pattern must start with this part
                    if !text.starts_with(part) {
                        return false;
                    }
                    pos = part.len();
                } else if i == parts.len() - 1 && !pattern.ends_with('%') {
                    // Pattern must end with this part
                    return text[pos..].ends_with(part);
                } else {
                    // Find this part in the remaining text
                    if let Some(idx) = text[pos..].find(part) {
                        pos += idx + part.len();
                    } else {
                        return false;
                    }
                }
            }
            true
        } else {
            text == pattern
        }
    }
}
