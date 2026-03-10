//! High-performance expression parser using nom
//!
//! Parses SQL-like transformation expressions into an AST.

use anyhow::{anyhow, Result};
use nom::{
    branch::alt,
    bytes::complete::{tag, tag_no_case, take_while, take_while1},
    character::complete::{char, digit1, multispace0, multispace1},
    combinator::{cut, map, opt, recognize, value},
    error::{context, VerboseError},
    multi::{separated_list0, separated_list1},
    sequence::{delimited, pair, preceded, terminated, tuple},
    IResult,
};

use super::ast::{BinaryOp, DataType, Expression, Literal, UnaryOp};

type ParseResult<'a, T> = IResult<&'a str, T, VerboseError<&'a str>>;

/// Expression parser using nom combinators
pub struct ExpressionParser {
    /// Whether to allow custom functions
    allow_custom_functions: bool,
}

impl ExpressionParser {
    /// Create a new parser with default settings
    pub fn new() -> Self {
        Self {
            allow_custom_functions: true,
        }
    }

    /// Parse a transformation expression into an AST
    pub fn parse(&self, input: &str) -> Result<Expression> {
        let (remaining, expr) = expression(input).map_err(|e| anyhow!("Parse error: {:?}", e))?;

        if !remaining.trim().is_empty() {
            return Err(anyhow!(
                "Unexpected input after expression: '{}'",
                remaining
            ));
        }

        Ok(expr)
    }
}

impl Default for ExpressionParser {
    fn default() -> Self {
        Self::new()
    }
}

// Primary expression parser
fn expression(input: &str) -> ParseResult<'_, Expression> {
    ws(or_expression)(input)
}

// Logical OR
fn or_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, first) = and_expression(input)?;

    let (input, rest) = many0(tuple((ws(tag_no_case("OR")), and_expression)))(input)?;

    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, (_, right)| Expression::BinaryOp {
                left: Box::new(acc),
                op: BinaryOp::Or,
                right: Box::new(right),
            }),
    ))
}

// Logical AND
fn and_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, first) = comparison_expression(input)?;

    let (input, rest) = many0(tuple((ws(tag_no_case("AND")), comparison_expression)))(input)?;

    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, (_, right)| Expression::BinaryOp {
                left: Box::new(acc),
                op: BinaryOp::And,
                right: Box::new(right),
            }),
    ))
}

// Comparison operators
fn comparison_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, left) = additive_expression(input)?;

    let (input, op_right) = opt(tuple((ws(comparison_op), additive_expression)))(input)?;

    if let Some((op, right)) = op_right {
        Ok((
            input,
            Expression::BinaryOp {
                left: Box::new(left),
                op,
                right: Box::new(right),
            },
        ))
    } else {
        Ok((input, left))
    }
}

// Parse comparison operators
fn comparison_op(input: &str) -> ParseResult<'_, BinaryOp> {
    alt((
        value(BinaryOp::LessThanOrEqual, tag("<=")),
        value(BinaryOp::GreaterThanOrEqual, tag(">=")),
        value(BinaryOp::NotEqual, alt((tag("!="), tag("<>")))),
        value(BinaryOp::Equal, tag("=")),
        value(BinaryOp::LessThan, tag("<")),
        value(BinaryOp::GreaterThan, tag(">")),
        value(BinaryOp::Like, tag_no_case("LIKE")),
        preceded(
            pair(tag_no_case("NOT"), multispace1),
            value(BinaryOp::NotLike, tag_no_case("LIKE")),
        ),
    ))(input)
}

// Addition and subtraction
fn additive_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, first) = multiplicative_expression(input)?;

    let (input, rest) = many0(tuple((
        ws(alt((
            value(BinaryOp::Add, tag("+")),
            value(BinaryOp::Subtract, tag("-")),
            value(BinaryOp::Concat, tag("||")),
        ))),
        multiplicative_expression,
    )))(input)?;

    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, (op, right)| Expression::BinaryOp {
                left: Box::new(acc),
                op,
                right: Box::new(right),
            }),
    ))
}

// Multiplication, division, modulo
fn multiplicative_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, first) = unary_expression(input)?;

    let (input, rest) = many0(tuple((
        ws(alt((
            value(BinaryOp::Multiply, tag("*")),
            value(BinaryOp::Divide, tag("/")),
            value(BinaryOp::Modulo, tag("%")),
        ))),
        unary_expression,
    )))(input)?;

    Ok((
        input,
        rest.into_iter()
            .fold(first, |acc, (op, right)| Expression::BinaryOp {
                left: Box::new(acc),
                op,
                right: Box::new(right),
            }),
    ))
}

// Unary operators
fn unary_expression(input: &str) -> ParseResult<'_, Expression> {
    alt((
        map(pair(ws(tag("-")), unary_expression), |(_, expr)| {
            Expression::UnaryOp {
                op: UnaryOp::Negate,
                expr: Box::new(expr),
            }
        }),
        map(
            pair(ws(tag_no_case("NOT")), unary_expression),
            |(_, expr)| Expression::UnaryOp {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            },
        ),
        postfix_expression,
    ))(input)
}

// Postfix operators (IS NULL, IS NOT NULL)
fn postfix_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, expr) = primary_expression(input)?;

    let (input, postfix) = opt(ws(alt((
        value(UnaryOp::IsNull, tag_no_case("IS NULL")),
        value(
            UnaryOp::IsNotNull,
            tuple((
                tag_no_case("IS"),
                multispace1,
                tag_no_case("NOT"),
                multispace1,
                tag_no_case("NULL"),
            )),
        ),
    ))))(input)?;

    if let Some(op) = postfix {
        Ok((
            input,
            Expression::UnaryOp {
                op,
                expr: Box::new(expr),
            },
        ))
    } else {
        Ok((input, expr))
    }
}

// Primary expressions
fn primary_expression(input: &str) -> ParseResult<'_, Expression> {
    ws(alt((
        // Parenthesized expression
        delimited(char('('), expression, char(')')),
        // CAST expression
        cast_expression,
        // CASE expression
        case_expression,
        // Function call
        function_call,
        // Variable
        variable,
        // Literal
        literal_expression,
    )))(input)
}

// CAST(expr AS type)
fn cast_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, _) = tag_no_case("CAST")(input)?;
    let (input, _) = ws(char('('))(input)?;
    let (input, expr) = expression(input)?;
    let (input, _) = ws(tag_no_case("AS"))(input)?;
    let (input, target_type) = ws(data_type)(input)?;
    let (input, _) = ws(char(')'))(input)?;

    Ok((
        input,
        Expression::Cast {
            expr: Box::new(expr),
            target_type,
        },
    ))
}

// CASE WHEN ... THEN ... ELSE ... END
fn case_expression(input: &str) -> ParseResult<'_, Expression> {
    let (input, _) = tag_no_case("CASE")(input)?;
    let (input, conditions) = many1(tuple((
        preceded(ws(tag_no_case("WHEN")), expression),
        preceded(ws(tag_no_case("THEN")), expression),
    )))(input)?;
    let (input, else_expr) = opt(preceded(ws(tag_no_case("ELSE")), expression))(input)?;
    let (input, _) = ws(tag_no_case("END"))(input)?;

    Ok((
        input,
        Expression::Case {
            conditions: conditions
                .into_iter()
                .map(|(cond, then_expr)| (Box::new(cond), Box::new(then_expr)))
                .collect(),
            else_expr: else_expr.map(Box::new),
        },
    ))
}

// Function calls
fn function_call(input: &str) -> ParseResult<'_, Expression> {
    let (input, name) = identifier(input)?;
    let (input, _) = ws(char('('))(input)?;
    let (input, args) = separated_list0(ws(char(',')), expression)(input)?;
    let (input, _) = ws(char(')'))(input)?;

    Ok((
        input,
        Expression::FunctionCall {
            name: name.to_uppercase(),
            args,
        },
    ))
}

// Variables: {field_name}
fn variable(input: &str) -> ParseResult<'_, Expression> {
    let (input, _) = char('{')(input)?;
    let (input, name) = take_while1(|c: char| c != '}')(input)?;
    let (input, _) = char('}')(input)?;

    Ok((input, Expression::Variable(name.to_string())))
}

// Literal values
fn literal_expression(input: &str) -> ParseResult<'_, Expression> {
    alt((
        map(null_literal, |_| Expression::Literal(Literal::Null)),
        map(boolean_literal, |b| {
            Expression::Literal(Literal::Boolean(b))
        }),
        map(float_literal, |f| Expression::Literal(Literal::Float(f))),
        map(integer_literal, |i| {
            Expression::Literal(Literal::Integer(i))
        }),
        map(string_literal, |s| Expression::Literal(Literal::String(s))),
    ))(input)
}

// NULL literal
fn null_literal(input: &str) -> ParseResult<'_, ()> {
    value((), tag_no_case("NULL"))(input)
}

// Boolean literals
fn boolean_literal(input: &str) -> ParseResult<'_, bool> {
    alt((
        value(true, tag_no_case("TRUE")),
        value(false, tag_no_case("FALSE")),
    ))(input)
}

// Integer literal
fn integer_literal(input: &str) -> ParseResult<'_, i64> {
    map(recognize(pair(opt(char('-')), digit1)), |s: &str| {
        s.parse::<i64>().unwrap()
    })(input)
}

// Float literal
fn float_literal(input: &str) -> ParseResult<'_, f64> {
    map(
        recognize(tuple((opt(char('-')), digit1, char('.'), digit1))),
        |s: &str| s.parse::<f64>().unwrap(),
    )(input)
}

// String literal (single or double quotes)
fn string_literal(input: &str) -> ParseResult<'_, String> {
    alt((
        delimited(
            char('\''),
            map(take_while(|c| c != '\''), String::from),
            char('\''),
        ),
        delimited(
            char('"'),
            map(take_while(|c| c != '"'), String::from),
            char('"'),
        ),
    ))(input)
}

// Identifier (function names, etc.)
fn identifier(input: &str) -> ParseResult<'_, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_')(input)
}

// Data type parsing
fn data_type(input: &str) -> ParseResult<'_, DataType> {
    alt((
        value(
            DataType::String,
            alt((
                tag_no_case("STRING"),
                tag_no_case("VARCHAR"),
                tag_no_case("TEXT"),
            )),
        ),
        value(
            DataType::Integer,
            alt((
                tag_no_case("INTEGER"),
                tag_no_case("INT"),
                tag_no_case("BIGINT"),
            )),
        ),
        value(
            DataType::Float,
            alt((
                tag_no_case("FLOAT"),
                tag_no_case("DOUBLE"),
                tag_no_case("REAL"),
            )),
        ),
        value(
            DataType::Boolean,
            alt((tag_no_case("BOOLEAN"), tag_no_case("BOOL"))),
        ),
        value(DataType::Date, tag_no_case("DATE")),
        value(DataType::Timestamp, tag_no_case("TIMESTAMP")),
        // DECIMAL(precision, scale)
        map(
            tuple((
                tag_no_case("DECIMAL"),
                opt(delimited(
                    ws(char('(')),
                    tuple((
                        map(digit1, |s: &str| s.parse::<u8>().unwrap()),
                        opt(preceded(
                            ws(char(',')),
                            map(digit1, |s: &str| s.parse::<u8>().unwrap()),
                        )),
                    )),
                    ws(char(')')),
                )),
            )),
            |(_, params)| {
                if let Some((precision, scale)) = params {
                    DataType::Decimal {
                        precision,
                        scale: scale.unwrap_or(0),
                    }
                } else {
                    DataType::Decimal {
                        precision: 38,
                        scale: 10,
                    }
                }
            },
        ),
    ))(input)
}

// Whitespace wrapper
fn ws<'a, O, F>(f: F) -> impl FnMut(&'a str) -> ParseResult<'a, O>
where
    F: FnMut(&'a str) -> ParseResult<'a, O>,
{
    delimited(multispace0, f, multispace0)
}

// Helper for many0 (zero or more)
fn many0<'a, O, F>(mut f: F) -> impl FnMut(&'a str) -> ParseResult<'a, Vec<O>>
where
    F: FnMut(&'a str) -> ParseResult<'a, O>,
{
    move |mut input| {
        let mut acc = Vec::new();
        while let Ok((i, o)) = f(input) {
            input = i;
            acc.push(o);
        }
        Ok((input, acc))
    }
}

// Helper for many1 (one or more)
fn many1<'a, O, F>(mut f: F) -> impl FnMut(&'a str) -> ParseResult<'a, Vec<O>>
where
    F: FnMut(&'a str) -> ParseResult<'a, O>,
{
    move |input| {
        let (input, first) = f(input)?;
        let (input, mut rest) = many0(&mut f)(input)?;
        rest.insert(0, first);
        Ok((input, rest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_variable() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("{field_name}").unwrap();
        assert!(matches!(expr, Expression::Variable(ref name) if name == "field_name"));
    }

    #[test]
    fn test_parse_string_literal() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("'hello world'").unwrap();
        assert!(matches!(
            expr,
            Expression::Literal(Literal::String(ref s)) if s == "hello world"
        ));
    }

    #[test]
    fn test_parse_function_call() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("UPPER({name})").unwrap();
        assert!(matches!(
            expr,
            Expression::FunctionCall { ref name, ref args } if name == "UPPER" && args.len() == 1
        ));
    }

    #[test]
    fn test_parse_nested_functions() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("UPPER(TRIM({name}))").unwrap();
        if let Expression::FunctionCall { name, args } = expr {
            assert_eq!(name, "UPPER");
            assert_eq!(args.len(), 1);
            if let Expression::FunctionCall { name, args } = &args[0] {
                assert_eq!(name, "TRIM");
                assert_eq!(args.len(), 1);
            }
        }
    }

    #[test]
    fn test_parse_cast() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("CAST({value} AS INTEGER)").unwrap();
        assert!(matches!(
            expr,
            Expression::Cast {
                target_type: DataType::Integer,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_binary_op() {
        let parser = ExpressionParser::new();
        let expr = parser.parse("{a} + {b}").unwrap();
        assert!(matches!(
            expr,
            Expression::BinaryOp {
                op: BinaryOp::Add,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_complex_expression() {
        let parser = ExpressionParser::new();
        let expr = parser
            .parse("UPPER(TRIM(COALESCE({first_name}, {last_name}, 'Unknown')))")
            .unwrap();
        assert!(matches!(expr, Expression::FunctionCall { .. }));
    }
}
