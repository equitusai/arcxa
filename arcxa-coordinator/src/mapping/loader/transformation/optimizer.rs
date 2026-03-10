//! Expression optimizer for transformation engine
//!
//! Optimizes AST expressions for better performance.

use anyhow::Result;
use std::collections::HashMap;

use super::ast::{BinaryOp, Expression, Literal, UnaryOp};

/// Expression optimizer
pub struct ExpressionOptimizer {
    /// Enable constant folding
    enable_constant_folding: bool,

    /// Enable algebraic simplification
    enable_algebraic_simplification: bool,

    /// Enable common subexpression elimination
    enable_cse: bool,
}

impl ExpressionOptimizer {
    /// Create a new optimizer with default settings
    pub fn new() -> Self {
        Self {
            enable_constant_folding: true,
            enable_algebraic_simplification: true,
            enable_cse: true,
        }
    }

    /// Optimize an expression
    pub fn optimize(&self, expr: Expression) -> Result<Expression> {
        let mut expr = expr;

        // Apply optimizations in order
        if self.enable_constant_folding {
            expr = self.fold_constants(expr)?;
        }

        if self.enable_algebraic_simplification {
            expr = self.simplify_algebraic(expr)?;
        }

        if self.enable_cse {
            expr = self.eliminate_common_subexpressions(expr)?;
        }

        Ok(expr)
    }

    /// Fold constant expressions
    fn fold_constants(&self, expr: Expression) -> Result<Expression> {
        match expr {
            Expression::BinaryOp { left, op, right } => {
                let left = Box::new(self.fold_constants(*left)?);
                let right = Box::new(self.fold_constants(*right)?);

                // Try to evaluate if both operands are literals
                match (&*left, &*right) {
                    (Expression::Literal(l), Expression::Literal(r)) => {
                        self.evaluate_binary_literals(l, &op, r)
                    }
                    _ => Ok(Expression::BinaryOp { left, op, right }),
                }
            }

            Expression::UnaryOp { op, expr } => {
                let expr = Box::new(self.fold_constants(*expr)?);

                // Try to evaluate if operand is literal
                match &*expr {
                    Expression::Literal(lit) => self.evaluate_unary_literal(&op, lit),
                    _ => Ok(Expression::UnaryOp { op, expr }),
                }
            }

            Expression::FunctionCall { name, args } => {
                let args = args
                    .into_iter()
                    .map(|arg| self.fold_constants(arg))
                    .collect::<Result<Vec<_>>>()?;

                // Some functions with literal arguments can be pre-computed
                // For now, just return the optimized function call
                Ok(Expression::FunctionCall { name, args })
            }

            Expression::Case {
                conditions,
                else_expr,
            } => {
                let conditions = conditions
                    .into_iter()
                    .map(|(cond, then_expr)| {
                        Ok((
                            Box::new(self.fold_constants(*cond)?),
                            Box::new(self.fold_constants(*then_expr)?),
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;

                let else_expr = if let Some(else_expr) = else_expr {
                    Some(Box::new(self.fold_constants(*else_expr)?))
                } else {
                    None
                };

                // Check if any conditions are constant true/false
                for (i, (cond, then_expr)) in conditions.iter().enumerate() {
                    if let Expression::Literal(Literal::Boolean(true)) = cond.as_ref() {
                        // This condition is always true, return the then branch
                        return Ok(then_expr.as_ref().clone());
                    }
                    if let Expression::Literal(Literal::Boolean(false)) = cond.as_ref() {
                        // This condition is always false, can be removed
                        // (handled by filtering below)
                    }
                }

                // Filter out always-false conditions
                let conditions: Vec<_> = conditions
                    .into_iter()
                    .filter(|(cond, _)| {
                        !matches!(cond.as_ref(), Expression::Literal(Literal::Boolean(false)))
                    })
                    .collect();

                if conditions.is_empty() {
                    // All conditions were false
                    if let Some(else_expr) = else_expr {
                        Ok(*else_expr)
                    } else {
                        Ok(Expression::Literal(Literal::Null))
                    }
                } else {
                    Ok(Expression::Case {
                        conditions,
                        else_expr,
                    })
                }
            }

            Expression::Cast { expr, target_type } => {
                let expr = Box::new(self.fold_constants(*expr)?);

                // If casting a literal, try to evaluate it
                // For now, just return the optimized cast
                Ok(Expression::Cast { expr, target_type })
            }

            _ => Ok(expr),
        }
    }

    /// Evaluate binary operation on literals
    fn evaluate_binary_literals(
        &self,
        left: &Literal,
        op: &BinaryOp,
        right: &Literal,
    ) -> Result<Expression> {
        let result = match (left, op, right) {
            // Integer arithmetic
            (Literal::Integer(l), BinaryOp::Add, Literal::Integer(r)) => Literal::Integer(l + r),
            (Literal::Integer(l), BinaryOp::Subtract, Literal::Integer(r)) => {
                Literal::Integer(l - r)
            }
            (Literal::Integer(l), BinaryOp::Multiply, Literal::Integer(r)) => {
                Literal::Integer(l * r)
            }
            (Literal::Integer(l), BinaryOp::Divide, Literal::Integer(r)) if *r != 0 => {
                Literal::Integer(l / r)
            }
            (Literal::Integer(l), BinaryOp::Modulo, Literal::Integer(r)) if *r != 0 => {
                Literal::Integer(l % r)
            }

            // Float arithmetic
            (Literal::Float(l), BinaryOp::Add, Literal::Float(r)) => Literal::Float(l + r),
            (Literal::Float(l), BinaryOp::Subtract, Literal::Float(r)) => Literal::Float(l - r),
            (Literal::Float(l), BinaryOp::Multiply, Literal::Float(r)) => Literal::Float(l * r),
            (Literal::Float(l), BinaryOp::Divide, Literal::Float(r)) if *r != 0.0 => {
                Literal::Float(l / r)
            }

            // String concatenation
            (Literal::String(l), BinaryOp::Concat, Literal::String(r)) => {
                Literal::String(format!("{}{}", l, r))
            }

            // Boolean logic
            (Literal::Boolean(l), BinaryOp::And, Literal::Boolean(r)) => Literal::Boolean(*l && *r),
            (Literal::Boolean(l), BinaryOp::Or, Literal::Boolean(r)) => Literal::Boolean(*l || *r),

            // Comparisons
            (Literal::Integer(l), BinaryOp::Equal, Literal::Integer(r)) => Literal::Boolean(l == r),
            (Literal::Integer(l), BinaryOp::NotEqual, Literal::Integer(r)) => {
                Literal::Boolean(l != r)
            }
            (Literal::Integer(l), BinaryOp::LessThan, Literal::Integer(r)) => {
                Literal::Boolean(l < r)
            }
            (Literal::Integer(l), BinaryOp::LessThanOrEqual, Literal::Integer(r)) => {
                Literal::Boolean(l <= r)
            }
            (Literal::Integer(l), BinaryOp::GreaterThan, Literal::Integer(r)) => {
                Literal::Boolean(l > r)
            }
            (Literal::Integer(l), BinaryOp::GreaterThanOrEqual, Literal::Integer(r)) => {
                Literal::Boolean(l >= r)
            }

            // Cannot evaluate at compile time, return as-is
            _ => {
                return Ok(Expression::BinaryOp {
                    left: Box::new(Expression::Literal(left.clone())),
                    op: op.clone(),
                    right: Box::new(Expression::Literal(right.clone())),
                });
            }
        };

        Ok(Expression::Literal(result))
    }

    /// Evaluate unary operation on literal
    fn evaluate_unary_literal(&self, op: &UnaryOp, lit: &Literal) -> Result<Expression> {
        let result = match (op, lit) {
            (UnaryOp::Negate, Literal::Integer(i)) => Literal::Integer(-i),
            (UnaryOp::Negate, Literal::Float(f)) => Literal::Float(-f),
            (UnaryOp::Not, Literal::Boolean(b)) => Literal::Boolean(!b),
            (UnaryOp::IsNull, Literal::Null) => Literal::Boolean(true),
            (UnaryOp::IsNull, _) => Literal::Boolean(false),
            (UnaryOp::IsNotNull, Literal::Null) => Literal::Boolean(false),
            (UnaryOp::IsNotNull, _) => Literal::Boolean(true),
            _ => {
                return Ok(Expression::UnaryOp {
                    op: op.clone(),
                    expr: Box::new(Expression::Literal(lit.clone())),
                });
            }
        };

        Ok(Expression::Literal(result))
    }

    /// Apply algebraic simplifications
    fn simplify_algebraic(&self, expr: Expression) -> Result<Expression> {
        match expr {
            Expression::BinaryOp { left, op, right } => {
                let left = Box::new(self.simplify_algebraic(*left)?);
                let right = Box::new(self.simplify_algebraic(*right)?);

                // Simplify based on operator and operands
                match (&op, &*left, &*right) {
                    // x + 0 = x
                    (BinaryOp::Add, _, Expression::Literal(Literal::Integer(0))) => {
                        Ok((*left).clone())
                    }
                    (BinaryOp::Add, Expression::Literal(Literal::Integer(0)), _) => {
                        Ok((*right).clone())
                    }

                    // x - 0 = x
                    (BinaryOp::Subtract, _, Expression::Literal(Literal::Integer(0))) => {
                        Ok((*left).clone())
                    }

                    // x * 0 = 0
                    (BinaryOp::Multiply, _, Expression::Literal(Literal::Integer(0)))
                    | (BinaryOp::Multiply, Expression::Literal(Literal::Integer(0)), _) => {
                        Ok(Expression::Literal(Literal::Integer(0)))
                    }

                    // x * 1 = x
                    (BinaryOp::Multiply, _, Expression::Literal(Literal::Integer(1))) => {
                        Ok((*left).clone())
                    }
                    (BinaryOp::Multiply, Expression::Literal(Literal::Integer(1)), _) => {
                        Ok((*right).clone())
                    }

                    // x / 1 = x
                    (BinaryOp::Divide, _, Expression::Literal(Literal::Integer(1))) => {
                        Ok((*left).clone())
                    }

                    // x AND true = x
                    (BinaryOp::And, _, Expression::Literal(Literal::Boolean(true))) => {
                        Ok((*left).clone())
                    }
                    (BinaryOp::And, Expression::Literal(Literal::Boolean(true)), _) => {
                        Ok((*right).clone())
                    }

                    // x AND false = false
                    (BinaryOp::And, _, Expression::Literal(Literal::Boolean(false)))
                    | (BinaryOp::And, Expression::Literal(Literal::Boolean(false)), _) => {
                        Ok(Expression::Literal(Literal::Boolean(false)))
                    }

                    // x OR true = true
                    (BinaryOp::Or, _, Expression::Literal(Literal::Boolean(true)))
                    | (BinaryOp::Or, Expression::Literal(Literal::Boolean(true)), _) => {
                        Ok(Expression::Literal(Literal::Boolean(true)))
                    }

                    // x OR false = x
                    (BinaryOp::Or, _, Expression::Literal(Literal::Boolean(false))) => {
                        Ok((*left).clone())
                    }
                    (BinaryOp::Or, Expression::Literal(Literal::Boolean(false)), _) => {
                        Ok((*right).clone())
                    }

                    // x = x => true (for simple variables)
                    (BinaryOp::Equal, Expression::Variable(l), Expression::Variable(r))
                        if l == r =>
                    {
                        Ok(Expression::Literal(Literal::Boolean(true)))
                    }

                    _ => Ok(Expression::BinaryOp { left, op, right }),
                }
            }

            Expression::UnaryOp { op, expr } => {
                let expr = Box::new(self.simplify_algebraic(*expr)?);

                match (&op, &*expr) {
                    // NOT NOT x = x
                    (
                        UnaryOp::Not,
                        Expression::UnaryOp {
                            op: UnaryOp::Not,
                            expr: inner,
                        },
                    ) => Ok(inner.as_ref().clone()),

                    // -(-x) = x
                    (
                        UnaryOp::Negate,
                        Expression::UnaryOp {
                            op: UnaryOp::Negate,
                            expr: inner,
                        },
                    ) => Ok(inner.as_ref().clone()),

                    _ => Ok(Expression::UnaryOp { op, expr }),
                }
            }

            Expression::FunctionCall { name, args } => {
                let args = args
                    .into_iter()
                    .map(|arg| self.simplify_algebraic(arg))
                    .collect::<Result<Vec<_>>>()?;

                // Special optimizations for specific functions
                match name.to_uppercase().as_str() {
                    "COALESCE" => {
                        // Remove NULL literals from COALESCE (except if all are NULL)
                        let non_null_args: Vec<_> = args
                            .into_iter()
                            .filter(|arg| !matches!(arg, Expression::Literal(Literal::Null)))
                            .collect();

                        if non_null_args.is_empty() {
                            Ok(Expression::Literal(Literal::Null))
                        } else if non_null_args.len() == 1 {
                            Ok(non_null_args.into_iter().next().unwrap())
                        } else {
                            Ok(Expression::FunctionCall {
                                name,
                                args: non_null_args,
                            })
                        }
                    }

                    _ => Ok(Expression::FunctionCall { name, args }),
                }
            }

            Expression::Case {
                conditions,
                else_expr,
            } => {
                let conditions = conditions
                    .into_iter()
                    .map(|(cond, then_expr)| {
                        Ok((
                            Box::new(self.simplify_algebraic(*cond)?),
                            Box::new(self.simplify_algebraic(*then_expr)?),
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;

                let else_expr = if let Some(else_expr) = else_expr {
                    Some(Box::new(self.simplify_algebraic(*else_expr)?))
                } else {
                    None
                };

                Ok(Expression::Case {
                    conditions,
                    else_expr,
                })
            }

            Expression::Cast { expr, target_type } => {
                let expr = Box::new(self.simplify_algebraic(*expr)?);
                Ok(Expression::Cast { expr, target_type })
            }

            _ => Ok(expr),
        }
    }

    /// Eliminate common subexpressions
    fn eliminate_common_subexpressions(&self, expr: Expression) -> Result<Expression> {
        // This is a simplified CSE implementation
        // A full implementation would:
        // 1. Build a DAG of expressions
        // 2. Find common subexpressions
        // 3. Replace duplicates with references
        // For now, just return the expression as-is
        Ok(expr)
    }
}

impl Default for ExpressionOptimizer {
    fn default() -> Self {
        Self::new()
    }
}
