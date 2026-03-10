//! Comprehensive test suite for the transformation engine

use anyhow::Result;
use graphica_coordinator::mapping::loader::transformation::*;
use std::collections::HashMap;

/// Helper function to create test context
fn create_test_context() -> HashMap<String, String> {
    let mut context = HashMap::new();
    context.insert("name".to_string(), "  John Doe  ".to_string());
    context.insert("email".to_string(), "JOHN@EXAMPLE.COM".to_string());
    context.insert("age".to_string(), "25".to_string());
    context.insert("salary".to_string(), "75000.50".to_string());
    context.insert("date".to_string(), "2024-01-15".to_string());
    context.insert("empty".to_string(), "".to_string());
    context.insert("null_field".to_string(), "".to_string());
    context
}

#[test]
fn test_string_functions() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // UPPER
    let result = engine.execute("UPPER({name})", &context)?;
    assert_eq!(result, Value::string_owned("  JOHN DOE  ".to_string()));

    // LOWER
    let result = engine.execute("LOWER({email})", &context)?;
    assert_eq!(result, Value::string_owned("john@example.com".to_string()));

    // TRIM
    let result = engine.execute("TRIM({name})", &context)?;
    assert_eq!(result, Value::string_owned("John Doe".to_string()));

    // LTRIM
    let result = engine.execute("LTRIM({name})", &context)?;
    assert_eq!(result, Value::string_owned("John Doe  ".to_string()));

    // RTRIM
    let result = engine.execute("RTRIM({name})", &context)?;
    assert_eq!(result, Value::string_owned("  John Doe".to_string()));

    // LENGTH
    let result = engine.execute("LENGTH(TRIM({name}))", &context)?;
    assert_eq!(result, Value::Integer(8)); // "John Doe" = 8 chars

    // SUBSTRING
    let result = engine.execute("SUBSTRING(TRIM({name}), 1, 4)", &context)?;
    assert_eq!(result, Value::string_owned("John".to_string()));

    // CONCAT
    let result = engine.execute("CONCAT({name}, ' - ', {email})", &context)?;
    assert_eq!(
        result,
        Value::string_owned("  John Doe   - JOHN@EXAMPLE.COM".to_string())
    );

    // REPLACE
    let result = engine.execute("REPLACE({name}, 'John', 'Jane')", &context)?;
    assert_eq!(result, Value::string_owned("  Jane Doe  ".to_string()));

    // LEFT
    let result = engine.execute("LEFT(TRIM({name}), 4)", &context)?;
    assert_eq!(result, Value::string_owned("John".to_string()));

    // RIGHT
    let result = engine.execute("RIGHT(TRIM({name}), 3)", &context)?;
    assert_eq!(result, Value::string_owned("Doe".to_string()));

    // REPEAT
    let result = engine.execute("REPEAT('X', 3)", &context)?;
    assert_eq!(result, Value::string_owned("XXX".to_string()));

    // REVERSE
    let result = engine.execute("REVERSE('hello')", &context)?;
    assert_eq!(result, Value::string_owned("olleh".to_string()));

    Ok(())
}

#[test]
fn test_numeric_functions() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = create_test_context();
    context.insert("negative".to_string(), "-42".to_string());
    context.insert("float".to_string(), "3.7".to_string());

    // ABS
    let result = engine.execute("ABS({negative})", &context)?;
    assert_eq!(result, Value::Float(42.0));

    // ROUND
    let result = engine.execute("ROUND({float})", &context)?;
    assert_eq!(result, Value::Float(4.0));

    let result = engine.execute("ROUND({salary}, 0)", &context)?;
    assert_eq!(result, Value::Float(75001.0));

    // FLOOR
    let result = engine.execute("FLOOR({float})", &context)?;
    assert_eq!(result, Value::Integer(3));

    // CEIL
    let result = engine.execute("CEIL({float})", &context)?;
    assert_eq!(result, Value::Integer(4));

    // POWER
    let result = engine.execute("POWER(2, 3)", &context)?;
    assert_eq!(result, Value::Float(8.0));

    // SQRT
    let result = engine.execute("SQRT(16)", &context)?;
    assert_eq!(result, Value::Float(4.0));

    // MOD
    let result = engine.execute("MOD(10, 3)", &context)?;
    assert_eq!(result, Value::Integer(1));

    Ok(())
}

#[test]
fn test_date_functions() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // DATE_FORMAT
    let result = engine.execute("DATE_FORMAT(CAST({date} AS DATE), 'MM/DD/YYYY')", &context)?;
    assert_eq!(result, Value::string_owned("01/15/2024".to_string()));

    // DATE_ADD
    let result = engine.execute("DATE_ADD(CAST({date} AS DATE), 7)", &context)?;
    if let Value::Date(d) = result {
        assert_eq!(d.to_string(), "2024-01-22");
    } else {
        panic!("Expected Date value");
    }

    Ok(())
}

#[test]
fn test_null_handling() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = create_test_context();
    context.insert("null1".to_string(), "".to_string());
    context.insert("value1".to_string(), "first".to_string());
    context.insert("value2".to_string(), "second".to_string());

    // COALESCE - treats empty string as null, returns first non-null/non-empty value
    let result = engine.execute("COALESCE({null1}, {value1}, {value2})", &context)?;
    assert_eq!(result, Value::string_owned("first".to_string())); // Returns first value since empty string is treated as null

    let result = engine.execute("COALESCE({value1}, {value2})", &context)?;
    assert_eq!(result, Value::string_owned("first".to_string()));

    // NULLIF
    let result = engine.execute("NULLIF({value1}, 'first')", &context)?;
    assert_eq!(result, Value::Null);

    let result = engine.execute("NULLIF({value1}, 'second')", &context)?;
    assert_eq!(result, Value::string_owned("first".to_string()));

    // IFNULL
    let result = engine.execute("IFNULL({null_field}, 'default')", &context)?;
    assert_eq!(result, Value::string_owned("".to_string()));

    Ok(())
}

#[test]
fn test_type_conversions() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // CAST to INTEGER
    let result = engine.execute("CAST({age} AS INTEGER)", &context)?;
    assert_eq!(result, Value::Integer(25));

    // CAST to FLOAT
    let result = engine.execute("CAST({salary} AS FLOAT)", &context)?;
    assert_eq!(result, Value::Float(75000.5));

    // CAST to STRING
    let result = engine.execute("CAST(123 AS STRING)", &context)?;
    assert_eq!(result, Value::string_owned("123".to_string()));

    // CAST to BOOLEAN
    let result = engine.execute("CAST(1 AS BOOLEAN)", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("CAST(0 AS BOOLEAN)", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // CAST to DATE
    let result = engine.execute("CAST({date} AS DATE)", &context)?;
    if let Value::Date(d) = result {
        assert_eq!(d.to_string(), "2024-01-15");
    } else {
        panic!("Expected Date value");
    }

    Ok(())
}

#[test]
fn test_conditional_expressions() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // Simple IF
    let result = engine.execute("IF({age} > '20', 'Adult', 'Minor')", &context)?;
    assert_eq!(result, Value::string_owned("Adult".to_string()));

    // CASE expression
    let result = engine.execute(
        "CASE WHEN {age} < '18' THEN 'Minor' WHEN {age} < '65' THEN 'Adult' ELSE 'Senior' END",
        &context,
    )?;
    assert_eq!(result, Value::string_owned("Adult".to_string()));

    // Nested CASE
    let result = engine.execute(
        "CASE \
         WHEN {age} < '30' THEN \
           CASE WHEN {salary} > '70000' THEN 'Young High Earner' ELSE 'Young' END \
         ELSE 'Other' \
         END",
        &context,
    )?;
    assert_eq!(result, Value::string_owned("Young High Earner".to_string()));

    Ok(())
}

#[test]
fn test_arithmetic_operations() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = HashMap::new();
    context.insert("a".to_string(), "10".to_string());
    context.insert("b".to_string(), "3".to_string());

    // Addition
    let result = engine.execute("{a} + {b}", &context)?;
    assert_eq!(result, Value::Float(13.0));

    // Subtraction
    let result = engine.execute("{a} - {b}", &context)?;
    assert_eq!(result, Value::Float(7.0));

    // Multiplication
    let result = engine.execute("{a} * {b}", &context)?;
    assert_eq!(result, Value::Float(30.0));

    // Division
    let result = engine.execute("{a} / {b}", &context)?;
    assert!(matches!(result, Value::Float(f) if (f - 3.333).abs() < 0.01));

    // Modulo - use MOD function for string values
    let result = engine.execute("MOD({a}, {b})", &context)?;
    assert_eq!(result, Value::Integer(1));

    // Complex expression
    let result = engine.execute("({a} + {b}) * 2 - 5", &context)?;
    assert_eq!(result, Value::Float(21.0));

    Ok(())
}

#[test]
fn test_comparison_operations() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = HashMap::new();
    context.insert("a".to_string(), "10".to_string());
    context.insert("b".to_string(), "3".to_string());
    context.insert("c".to_string(), "10".to_string());

    // Equal
    let result = engine.execute("{a} = {c}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("{a} = {b}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // Not equal
    let result = engine.execute("{a} != {b}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    // Less than (string comparison: "3" > "10" lexicographically)
    let result = engine.execute("{b} < {a}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // Less than or equal
    let result = engine.execute("{a} <= {c}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    // Greater than (string comparison: "10" < "3" lexicographically)
    let result = engine.execute("{a} > {b}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // Greater than or equal
    let result = engine.execute("{a} >= {c}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    Ok(())
}

#[test]
fn test_logical_operations() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = HashMap::new();
    context.insert("true_val".to_string(), "1".to_string());
    context.insert("false_val".to_string(), "0".to_string());

    // AND
    let result = engine.execute("{true_val} AND {true_val}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("{true_val} AND {false_val}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // OR
    let result = engine.execute("{true_val} OR {false_val}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("{false_val} OR {false_val}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    // NOT
    let result = engine.execute("NOT {true_val}", &context)?;
    assert_eq!(result, Value::Boolean(false));

    let result = engine.execute("NOT {false_val}", &context)?;
    assert_eq!(result, Value::Boolean(true));

    Ok(())
}

#[test]
fn test_pattern_matching() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = HashMap::new();
    context.insert("text".to_string(), "hello world".to_string());

    // LIKE with %
    let result = engine.execute("'{text}' LIKE 'hello%'", &context)?;
    assert_eq!(result, Value::Boolean(false)); // Because {text} is not expanded in string literal

    // Proper way with variable
    context.insert("pattern".to_string(), "hello%".to_string());
    let result = engine.execute("{text} LIKE 'hello%'", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("{text} LIKE '%world'", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("{text} LIKE '%lo wo%'", &context)?;
    assert_eq!(result, Value::Boolean(true));

    // NOT LIKE
    let result = engine.execute("{text} NOT LIKE 'goodbye%'", &context)?;
    assert_eq!(result, Value::Boolean(true));

    Ok(())
}

#[test]
fn test_regex_functions() -> Result<()> {
    let engine = TransformationEngine::new();
    let mut context = HashMap::new();
    context.insert("email".to_string(), "john@example.com".to_string());
    context.insert("phone".to_string(), "123-456-7890".to_string());

    // REGEX_MATCH
    let result = engine.execute("REGEX_MATCH({email}, '^[a-z]+@[a-z]+\\.[a-z]+$')", &context)?;
    assert_eq!(result, Value::Boolean(true));

    let result = engine.execute("REGEX_MATCH({phone}, '^\\d{3}-\\d{3}-\\d{4}$')", &context)?;
    assert_eq!(result, Value::Boolean(true));

    // REGEX_REPLACE
    let result = engine.execute("REGEX_REPLACE({phone}, '-', '')", &context)?;
    assert_eq!(result, Value::string_owned("1234567890".to_string()));

    Ok(())
}

#[test]
fn test_complex_nested_expressions() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // Complex nested expression - UPPER applies to entire CONCAT result
    let result = engine.execute(
        "UPPER(CONCAT(TRIM({name}), ' (', LOWER(TRIM({email})), ') - Age: ', {age}))",
        &context,
    )?;
    assert_eq!(
        result,
        Value::string_owned("JOHN DOE (JOHN@EXAMPLE.COM) - AGE: 25".to_string())
    );

    // Nested function calls with arithmetic
    let result = engine.execute(
        "CONCAT('Total: $', ROUND(CAST({salary} AS FLOAT) * 1.1, 2))",
        &context,
    )?;
    assert_eq!(result, Value::string_owned("Total: $82500.55".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_batch_processing() -> Result<()> {
    let engine = TransformationEngine::new();

    let batch: Vec<_> = (0..100)
        .map(|i| {
            let mut row = HashMap::new();
            row.insert("id".to_string(), i.to_string());
            row.insert("name".to_string(), format!("  Person {}  ", i));
            row
        })
        .collect();

    let results = engine
        .execute_batch("UPPER(TRIM({name}))", batch.clone())
        .await?;

    assert_eq!(results.len(), 100);

    for (i, result) in results.iter().enumerate() {
        match result {
            Ok(Value::String(s)) => {
                assert_eq!(s.as_ref(), &format!("PERSON {}", i));
            }
            _ => panic!("Unexpected result"),
        }
    }

    Ok(())
}

#[test]
fn test_error_handling() -> Result<()> {
    let engine = TransformationEngine::new();
    let context = create_test_context();

    // Division by zero
    assert!(engine.execute("10 / 0", &context).is_err());

    // Invalid function
    assert!(engine.execute("INVALID_FUNC({name})", &context).is_err());

    // Type mismatch
    assert!(engine
        .execute("CAST('not-a-number' AS INTEGER)", &context)
        .is_err());

    // Invalid regex
    assert!(engine
        .execute("REGEX_MATCH({name}, '[invalid')", &context)
        .is_err());

    // Missing variable
    assert!(engine.execute("{non_existent_field}", &context).is_err());

    Ok(())
}

#[test]
fn test_optimizer() -> Result<()> {
    let optimizer = ExpressionOptimizer::new();
    let parser = ExpressionParser::new();

    // Constant folding
    let expr = parser.parse("1 + 2 + 3")?;
    let optimized = optimizer.optimize(expr)?;
    assert!(matches!(optimized, Expression::Literal(_)));

    // Algebraic simplification
    let expr = parser.parse("{x} + 0")?;
    let optimized = optimizer.optimize(expr)?;
    assert!(matches!(optimized, Expression::Variable(_)));

    let expr = parser.parse("{x} * 1")?;
    let optimized = optimizer.optimize(expr)?;
    assert!(matches!(optimized, Expression::Variable(_)));

    Ok(())
}

#[test]
fn test_plan_cache() -> Result<()> {
    let cache = PlanCache::new(10);
    let expr = Expression::Variable("test".to_string());
    let functions = std::sync::Arc::new(FunctionRegistry::with_builtins());
    let plan = ExecutionPlan::from_ast(expr, &functions)?;

    // Test insert and retrieve
    cache.insert("test_expr".to_string(), plan.clone());
    assert_eq!(cache.len(), 1);

    let retrieved = cache.get("test_expr");
    assert!(retrieved.is_some());

    // Test cache miss
    assert!(cache.get("non_existent").is_none());

    // Test clear
    cache.clear();
    assert_eq!(cache.len(), 0);

    Ok(())
}
