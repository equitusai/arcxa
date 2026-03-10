//! Function registry for transformation engine

use anyhow::{anyhow, Result};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Timelike};
use chrono_tz::Tz;
use regex::Regex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use super::types::Value;

/// Transformation function trait
pub trait TransformFunction: Send + Sync {
    /// Execute the function with given arguments
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

    /// Check if function is deterministic (same input = same output)
    fn is_deterministic(&self) -> bool {
        true
    }
}

/// Function registry
pub struct FunctionRegistry {
    functions: HashMap<String, Arc<dyn TransformFunction>>,
}

impl FunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }

    /// Create a registry with all built-in functions
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // String functions
        registry.register("UPPER", Arc::new(UpperFunction));
        registry.register("LOWER", Arc::new(LowerFunction));
        registry.register("TRIM", Arc::new(TrimFunction));
        registry.register("LTRIM", Arc::new(LTrimFunction));
        registry.register("RTRIM", Arc::new(RTrimFunction));
        registry.register("LENGTH", Arc::new(LengthFunction));
        registry.register("SUBSTRING", Arc::new(SubstringFunction));
        registry.register("REPLACE", Arc::new(ReplaceFunction));
        registry.register("CONCAT", Arc::new(ConcatFunction));
        registry.register("LEFT", Arc::new(LeftFunction));
        registry.register("RIGHT", Arc::new(RightFunction));
        registry.register("REPEAT", Arc::new(RepeatFunction));
        registry.register("REVERSE", Arc::new(ReverseFunction));
        registry.register("SPLIT_PART", Arc::new(SplitPartFunction));

        // Numeric functions
        registry.register("ABS", Arc::new(AbsFunction));
        registry.register("ROUND", Arc::new(RoundFunction));
        registry.register("FLOOR", Arc::new(FloorFunction));
        registry.register("CEIL", Arc::new(CeilFunction));
        registry.register("POWER", Arc::new(PowerFunction));
        registry.register("SQRT", Arc::new(SqrtFunction));
        registry.register("MOD", Arc::new(ModFunction));

        // Date functions
        registry.register("CURRENT_DATE", Arc::new(CurrentDateFunction));
        registry.register("DATE_ADD", Arc::new(DateAddFunction));
        registry.register("DATE_DIFF", Arc::new(DateDiffFunction));
        registry.register("DATE_FORMAT", Arc::new(DateFormatFunction));

        // NULL handling
        registry.register("COALESCE", Arc::new(CoalesceFunction));
        registry.register("NULLIF", Arc::new(NullIfFunction));
        registry.register("IFNULL", Arc::new(IfNullFunction));

        // Type conversion (handled specially in executor, but register for completeness)
        registry.register("CAST", Arc::new(CastFunction));

        // Conditional
        registry.register("IF", Arc::new(IfFunction));

        // Regex
        registry.register("REGEX_MATCH", Arc::new(RegexMatchFunction));
        registry.register("REGEX_REPLACE", Arc::new(RegexReplaceFunction));
        registry.register("REGEX_EXTRACT", Arc::new(RegexExtractFunction));
        registry.register("REGEX_SPLIT", Arc::new(RegexSplitFunction));
        registry.register("REGEX_EXTRACT_ALL", Arc::new(RegexExtractAllFunction));

        // Additional Date/Time functions
        registry.register("PARSE_DATE", Arc::new(ParseDateFunction));
        registry.register("FORMAT_DATE", Arc::new(FormatDateFunction));
        registry.register("EXTRACT", Arc::new(ExtractFunction));
        registry.register("NOW", Arc::new(NowFunction));
        registry.register("DATE_TRUNC", Arc::new(DateTruncFunction));
        registry.register("IS_VALID_DATE", Arc::new(IsValidDateFunction));

        // Conditional functions
        registry.register("CASE", Arc::new(CaseFunction));
        registry.register("IS_NULL", Arc::new(IsNullFunction));
        registry.register("IS_NOT_NULL", Arc::new(IsNotNullFunction));
        registry.register("IS_EMPTY", Arc::new(IsEmptyFunction));
        registry.register("IS_NUMERIC", Arc::new(IsNumericFunction));
        registry.register("IS_EMAIL", Arc::new(IsEmailFunction));
        registry.register("IS_URL", Arc::new(IsUrlFunction));
        registry.register("AND", Arc::new(AndFunction));
        registry.register("OR", Arc::new(OrFunction));
        registry.register("NOT", Arc::new(NotFunction));

        // Enhanced String functions
        registry.register("CAPITALIZE", Arc::new(CapitalizeFunction));
        registry.register("TITLE_CASE", Arc::new(TitleCaseFunction));
        registry.register("SLUG", Arc::new(SlugFunction));
        registry.register("REMOVE_ACCENTS", Arc::new(RemoveAccentsFunction));
        registry.register("TRANSLITERATE", Arc::new(TransliterateFunction));
        registry.register("MASK", Arc::new(MaskFunction));
        registry.register("TRUNCATE", Arc::new(TruncateFunction));

        registry
    }

    /// Register a function
    pub fn register(&mut self, name: &str, func: Arc<dyn TransformFunction>) {
        self.functions.insert(name.to_uppercase(), func);
    }

    /// Execute a function by name
    pub fn execute(&self, name: &str, args: &[Value]) -> Result<Value> {
        let func = self
            .functions
            .get(&name.to_uppercase())
            .ok_or_else(|| anyhow!("Unknown function: {}", name))?;

        // Check argument count
        if args.len() < func.min_args() {
            return Err(anyhow!(
                "{} requires at least {} arguments, got {}",
                name,
                func.min_args(),
                args.len()
            ));
        }

        if let Some(max) = func.max_args() {
            if args.len() > max {
                return Err(anyhow!(
                    "{} accepts at most {} arguments, got {}",
                    name,
                    max,
                    args.len()
                ));
            }
        }

        func.execute(args)
    }

    /// Check if function exists
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(&name.to_uppercase())
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

// String functions

struct UpperFunction;
impl TransformFunction for UpperFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::string_owned(s.to_uppercase()))
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

struct LowerFunction;
impl TransformFunction for LowerFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::string_owned(s.to_lowercase()))
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

struct TrimFunction;
impl TransformFunction for TrimFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::string_owned(s.trim().to_string()))
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

struct LTrimFunction;
impl TransformFunction for LTrimFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::string_owned(s.trim_start().to_string()))
    }

    fn name(&self) -> &str {
        "LTRIM"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct RTrimFunction;
impl TransformFunction for RTrimFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::string_owned(s.trim_end().to_string()))
    }

    fn name(&self) -> &str {
        "RTRIM"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct LengthFunction;
impl TransformFunction for LengthFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let s = args[0].as_string();
        Ok(Value::Integer(s.len() as i64))
    }

    fn name(&self) -> &str {
        "LENGTH"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct SubstringFunction;
impl TransformFunction for SubstringFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let s = args[0].as_string();
        let start = args[1].as_integer()? as usize;
        let start = if start > 0 { start - 1 } else { 0 }; // SQL uses 1-based indexing

        let result = if args.len() > 2 {
            let length = args[2].as_integer()? as usize;
            s.chars().skip(start).take(length).collect()
        } else {
            s.chars().skip(start).collect()
        };

        Ok(Value::string_owned(result))
    }

    fn name(&self) -> &str {
        "SUBSTRING"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}

struct ReplaceFunction;
impl TransformFunction for ReplaceFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let from = args[1].as_string();
        let to = args[2].as_string();

        Ok(Value::string_owned(
            text.replace(from.as_ref(), to.as_ref()),
        ))
    }

    fn name(&self) -> &str {
        "REPLACE"
    }

    fn min_args(&self) -> usize {
        3
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}

struct ConcatFunction;
impl TransformFunction for ConcatFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        let mut result = String::new();
        for arg in args {
            if !arg.is_null() {
                result.push_str(&arg.as_string());
            }
        }
        Ok(Value::string_owned(result))
    }

    fn name(&self) -> &str {
        "CONCAT"
    }

    fn min_args(&self) -> usize {
        1
    }
}

struct LeftFunction;
impl TransformFunction for LeftFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let s = args[0].as_string();
        let n = args[1].as_integer()? as usize;

        Ok(Value::string_owned(s.chars().take(n).collect()))
    }

    fn name(&self) -> &str {
        "LEFT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct RightFunction;
impl TransformFunction for RightFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let s = args[0].as_string();
        let n = args[1].as_integer()? as usize;
        let len = s.chars().count();

        if n >= len {
            Ok(Value::string_owned(s.into_owned()))
        } else {
            Ok(Value::string_owned(s.chars().skip(len - n).collect()))
        }
    }

    fn name(&self) -> &str {
        "RIGHT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct RepeatFunction;
impl TransformFunction for RepeatFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let s = args[0].as_string();
        let n = args[1].as_integer()? as usize;

        Ok(Value::string_owned(s.repeat(n)))
    }

    fn name(&self) -> &str {
        "REPEAT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct ReverseFunction;
impl TransformFunction for ReverseFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let s = args[0].as_string();
        Ok(Value::string_owned(s.chars().rev().collect()))
    }

    fn name(&self) -> &str {
        "REVERSE"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// SPLIT_PART(string, delimiter, index)
/// Splits string by delimiter and returns the part at the given index (1-based)
struct SplitPartFunction;
impl TransformFunction for SplitPartFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let string = args[0].as_string();
        let delimiter = args[1].as_string();
        let index = args[2].as_integer()? as usize;

        if index == 0 {
            return Err(anyhow!("SPLIT_PART index must be >= 1 (1-based indexing)"));
        }

        let parts: Vec<&str> = string.split(delimiter.as_ref()).collect();

        // 1-based indexing (like SQL)
        if index > parts.len() {
            // Return empty string if index is out of bounds (SQL behavior)
            Ok(Value::string_owned(String::new()))
        } else {
            Ok(Value::string_owned(parts[index - 1].to_string()))
        }
    }

    fn name(&self) -> &str {
        "SPLIT_PART"
    }

    fn min_args(&self) -> usize {
        3
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}

// Numeric functions

struct AbsFunction;
impl TransformFunction for AbsFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        match &args[0] {
            Value::Null => Ok(Value::Null),
            Value::Integer(i) => Ok(Value::Integer(i.abs())),
            Value::Float(f) => Ok(Value::Float(f.abs())),
            _ => {
                let f = args[0].as_float()?;
                Ok(Value::Float(f.abs()))
            }
        }
    }

    fn name(&self) -> &str {
        "ABS"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct RoundFunction;
impl TransformFunction for RoundFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let value = args[0].as_float()?;
        let decimals = if args.len() > 1 {
            args[1].as_integer()? as i32
        } else {
            0
        };

        let multiplier = 10_f64.powi(decimals);
        Ok(Value::Float((value * multiplier).round() / multiplier))
    }

    fn name(&self) -> &str {
        "ROUND"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct FloorFunction;
impl TransformFunction for FloorFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let f = args[0].as_float()?;
        Ok(Value::Integer(f.floor() as i64))
    }

    fn name(&self) -> &str {
        "FLOOR"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct CeilFunction;
impl TransformFunction for CeilFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let f = args[0].as_float()?;
        Ok(Value::Integer(f.ceil() as i64))
    }

    fn name(&self) -> &str {
        "CEIL"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct PowerFunction;
impl TransformFunction for PowerFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }
        let base = args[0].as_float()?;
        let exponent = args[1].as_float()?;
        Ok(Value::Float(base.powf(exponent)))
    }

    fn name(&self) -> &str {
        "POWER"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct SqrtFunction;
impl TransformFunction for SqrtFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }
        let f = args[0].as_float()?;
        if f < 0.0 {
            return Err(anyhow!("Cannot take square root of negative number"));
        }
        Ok(Value::Float(f.sqrt()))
    }

    fn name(&self) -> &str {
        "SQRT"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

struct ModFunction;
impl TransformFunction for ModFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }
        let a = args[0].as_integer()?;
        let b = args[1].as_integer()?;
        if b == 0 {
            return Err(anyhow!("Modulo by zero"));
        }
        Ok(Value::Integer(a % b))
    }

    fn name(&self) -> &str {
        "MOD"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

// Date functions

struct CurrentDateFunction;
impl TransformFunction for CurrentDateFunction {
    fn execute(&self, _args: &[Value]) -> Result<Value> {
        Ok(Value::Date(chrono::Local::now().naive_local().date()))
    }

    fn name(&self) -> &str {
        "CURRENT_DATE"
    }

    fn is_deterministic(&self) -> bool {
        false
    }

    fn max_args(&self) -> Option<usize> {
        Some(0)
    }
}

struct DateAddFunction;
impl TransformFunction for DateAddFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let date = args[0].as_date()?;
        let days = args[1].as_integer()?;

        Ok(Value::Date(date + Duration::days(days)))
    }

    fn name(&self) -> &str {
        "DATE_ADD"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct DateDiffFunction;
impl TransformFunction for DateDiffFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }

        let date1 = args[0].as_date()?;
        let date2 = args[1].as_date()?;

        Ok(Value::Integer((date1 - date2).num_days()))
    }

    fn name(&self) -> &str {
        "DATE_DIFF"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct DateFormatFunction;
impl TransformFunction for DateFormatFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let date = args[0].as_date()?;
        let format = args[1].as_string();

        // Simple format support (extend as needed)
        let formatted = match format.as_ref() {
            "YYYY-MM-DD" => date.format("%Y-%m-%d").to_string(),
            "MM/DD/YYYY" => date.format("%m/%d/%Y").to_string(),
            "DD/MM/YYYY" => date.format("%d/%m/%Y").to_string(),
            _ => date.format(&format).to_string(),
        };

        Ok(Value::string_owned(formatted))
    }

    fn name(&self) -> &str {
        "DATE_FORMAT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

// NULL handling functions

struct CoalesceFunction;
impl TransformFunction for CoalesceFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        for arg in args {
            // COALESCE returns the first non-null AND non-empty value
            if !arg.is_null() {
                // Also check if it's an empty string
                if let Value::String(s) = arg {
                    if !s.is_empty() {
                        return Ok(arg.clone());
                    }
                } else {
                    return Ok(arg.clone());
                }
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

struct NullIfFunction;
impl TransformFunction for NullIfFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0] == args[1] {
            Ok(Value::Null)
        } else {
            Ok(args[0].clone())
        }
    }

    fn name(&self) -> &str {
        "NULLIF"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct IfNullFunction;
impl TransformFunction for IfNullFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            Ok(args[1].clone())
        } else {
            Ok(args[0].clone())
        }
    }

    fn name(&self) -> &str {
        "IFNULL"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

// Type conversion

struct CastFunction;
impl TransformFunction for CastFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        // CAST is handled specially in the executor
        // This is just a placeholder
        Ok(args[0].clone())
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

// Conditional

struct IfFunction;
impl TransformFunction for IfFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        let condition = args[0].as_boolean();
        if condition {
            Ok(args[1].clone())
        } else {
            Ok(args[2].clone())
        }
    }

    fn name(&self) -> &str {
        "IF"
    }

    fn min_args(&self) -> usize {
        3
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}

// Regex functions

struct RegexMatchFunction;
impl TransformFunction for RegexMatchFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let pattern = args[1].as_string();

        let re = Regex::new(&pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

        Ok(Value::Boolean(re.is_match(&text)))
    }

    fn name(&self) -> &str {
        "REGEX_MATCH"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

struct RegexReplaceFunction;
impl TransformFunction for RegexReplaceFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let pattern = args[1].as_string();
        let replacement = args[2].as_string();

        let re = Regex::new(&pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

        Ok(Value::string_owned(
            re.replace_all(&text, replacement.as_ref()).into_owned(),
        ))
    }

    fn name(&self) -> &str {
        "REGEX_REPLACE"
    }

    fn min_args(&self) -> usize {
        3
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}

// Additional Regex functions

/// REGEX_EXTRACT(text, pattern) - Extract first match
struct RegexExtractFunction;
impl TransformFunction for RegexExtractFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let pattern = args[1].as_string();

        let re = Regex::new(&pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

        match re.captures(&text) {
            Some(caps) => {
                // Return first capture group, or full match if no groups
                let result = if caps.len() > 1 {
                    caps.get(1).map(|m| m.as_str()).unwrap_or("")
                } else {
                    caps.get(0).map(|m| m.as_str()).unwrap_or("")
                };
                Ok(Value::string_owned(result.to_string()))
            }
            None => Ok(Value::Null),
        }
    }

    fn name(&self) -> &str {
        "REGEX_EXTRACT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// REGEX_SPLIT(text, pattern) - Split text by regex pattern, return array
struct RegexSplitFunction;
impl TransformFunction for RegexSplitFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let pattern = args[1].as_string();

        let re = Regex::new(&pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

        let parts: Vec<Value> = re
            .split(&text)
            .map(|s| Value::string_owned(s.to_string()))
            .collect();

        Ok(Value::Array(parts))
    }

    fn name(&self) -> &str {
        "REGEX_SPLIT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// REGEX_EXTRACT_ALL(text, pattern) - Extract all matches, return array
struct RegexExtractAllFunction;
impl TransformFunction for RegexExtractAllFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() || args[1].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let pattern = args[1].as_string();

        let re = Regex::new(&pattern).map_err(|e| anyhow!("Invalid regex: {}", e))?;

        let matches: Vec<Value> = re
            .find_iter(&text)
            .map(|m| Value::string_owned(m.as_str().to_string()))
            .collect();

        Ok(Value::Array(matches))
    }

    fn name(&self) -> &str {
        "REGEX_EXTRACT_ALL"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

// Additional Date/Time functions

/// PARSE_DATE(text, format) - Parse string to date
struct ParseDateFunction;
impl TransformFunction for ParseDateFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let format = args[1].as_string();

        let date = NaiveDate::parse_from_str(&text, &format).map_err(|e| {
            anyhow!(
                "Failed to parse date '{}' with format '{}': {}",
                text,
                format,
                e
            )
        })?;

        Ok(Value::Date(date))
    }

    fn name(&self) -> &str {
        "PARSE_DATE"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// FORMAT_DATE(date, format) - Format date to string (alias for DATE_FORMAT)
struct FormatDateFunction;
impl TransformFunction for FormatDateFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let date = args[0].as_date()?;
        let format = args[1].as_string();

        Ok(Value::string_owned(date.format(&format).to_string()))
    }

    fn name(&self) -> &str {
        "FORMAT_DATE"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// EXTRACT(unit, date) - Extract date component (year, month, day, etc.)
struct ExtractFunction;
impl TransformFunction for ExtractFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        let unit = args[0].as_string();

        if args[1].is_null() {
            return Ok(Value::Null);
        }

        match &args[1] {
            Value::Date(d) => {
                let result = match unit.to_uppercase().as_str() {
                    "YEAR" => d.year() as i64,
                    "MONTH" => d.month() as i64,
                    "DAY" => d.day() as i64,
                    "DOW" | "DAYOFWEEK" => d.weekday().number_from_monday() as i64,
                    "DOY" | "DAYOFYEAR" => d.ordinal() as i64,
                    _ => return Err(anyhow!("Unknown date unit: {}", unit)),
                };
                Ok(Value::Integer(result))
            }
            Value::Timestamp(t) => {
                let result = match unit.to_uppercase().as_str() {
                    "YEAR" => t.year() as i64,
                    "MONTH" => t.month() as i64,
                    "DAY" => t.day() as i64,
                    "HOUR" => t.hour() as i64,
                    "MINUTE" => t.minute() as i64,
                    "SECOND" => t.second() as i64,
                    "DOW" | "DAYOFWEEK" => t.weekday().number_from_monday() as i64,
                    "DOY" | "DAYOFYEAR" => t.ordinal() as i64,
                    _ => return Err(anyhow!("Unknown timestamp unit: {}", unit)),
                };
                Ok(Value::Integer(result))
            }
            _ => Err(anyhow!("EXTRACT requires a Date or Timestamp value")),
        }
    }

    fn name(&self) -> &str {
        "EXTRACT"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// NOW() - Get current timestamp
struct NowFunction;
impl TransformFunction for NowFunction {
    fn execute(&self, _args: &[Value]) -> Result<Value> {
        Ok(Value::Timestamp(chrono::Local::now().naive_local()))
    }

    fn name(&self) -> &str {
        "NOW"
    }

    fn is_deterministic(&self) -> bool {
        false
    }

    fn max_args(&self) -> Option<usize> {
        Some(0)
    }
}

/// DATE_TRUNC(unit, date) - Truncate date to precision
struct DateTruncFunction;
impl TransformFunction for DateTruncFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        let unit = args[0].as_string();

        if args[1].is_null() {
            return Ok(Value::Null);
        }

        match &args[1] {
            Value::Date(d) => {
                let result = match unit.to_uppercase().as_str() {
                    "YEAR" => NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap(),
                    "MONTH" => NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap(),
                    "DAY" => *d,
                    _ => return Err(anyhow!("Unknown truncation unit: {}", unit)),
                };
                Ok(Value::Date(result))
            }
            Value::Timestamp(t) => {
                let result = match unit.to_uppercase().as_str() {
                    "YEAR" => NaiveDateTime::new(
                        NaiveDate::from_ymd_opt(t.year(), 1, 1).unwrap(),
                        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                    ),
                    "MONTH" => NaiveDateTime::new(
                        NaiveDate::from_ymd_opt(t.year(), t.month(), 1).unwrap(),
                        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                    ),
                    "DAY" => NaiveDateTime::new(
                        t.date(),
                        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap(),
                    ),
                    "HOUR" => NaiveDateTime::new(
                        t.date(),
                        chrono::NaiveTime::from_hms_opt(t.hour(), 0, 0).unwrap(),
                    ),
                    "MINUTE" => NaiveDateTime::new(
                        t.date(),
                        chrono::NaiveTime::from_hms_opt(t.hour(), t.minute(), 0).unwrap(),
                    ),
                    _ => return Err(anyhow!("Unknown truncation unit: {}", unit)),
                };
                Ok(Value::Timestamp(result))
            }
            _ => Err(anyhow!("DATE_TRUNC requires a Date or Timestamp value")),
        }
    }

    fn name(&self) -> &str {
        "DATE_TRUNC"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

/// IS_VALID_DATE(text, format) - Check if text is a valid date
struct IsValidDateFunction;
impl TransformFunction for IsValidDateFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Boolean(false));
        }

        let text = args[0].as_string();
        let format = args[1].as_string();

        let is_valid = NaiveDate::parse_from_str(&text, &format).is_ok();
        Ok(Value::Boolean(is_valid))
    }

    fn name(&self) -> &str {
        "IS_VALID_DATE"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(2)
    }
}

// Conditional functions

/// CASE(condition1, value1, condition2, value2, ..., else_value)
/// SQL-like CASE WHEN expression
struct CaseFunction;
impl TransformFunction for CaseFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args.len() < 3 || args.len() % 2 == 0 {
            return Err(anyhow!(
                "CASE requires odd number of arguments: condition1, value1, ..., else_value"
            ));
        }

        // Check pairs of (condition, value)
        for i in (0..args.len() - 1).step_by(2) {
            let condition = args[i].as_boolean();
            if condition {
                return Ok(args[i + 1].clone());
            }
        }

        // Return else value (last argument)
        Ok(args[args.len() - 1].clone())
    }

    fn name(&self) -> &str {
        "CASE"
    }

    fn min_args(&self) -> usize {
        3
    }
}

/// IS_NULL(value) - Check if value is null
struct IsNullFunction;
impl TransformFunction for IsNullFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        Ok(Value::Boolean(args[0].is_null()))
    }

    fn name(&self) -> &str {
        "IS_NULL"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// IS_NOT_NULL(value) - Check if value is not null
struct IsNotNullFunction;
impl TransformFunction for IsNotNullFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        Ok(Value::Boolean(!args[0].is_null()))
    }

    fn name(&self) -> &str {
        "IS_NOT_NULL"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// IS_EMPTY(value) - Check if string is empty
struct IsEmptyFunction;
impl TransformFunction for IsEmptyFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Boolean(true));
        }

        match &args[0] {
            Value::String(s) => Ok(Value::Boolean(s.is_empty())),
            Value::Array(a) => Ok(Value::Boolean(a.is_empty())),
            _ => Ok(Value::Boolean(false)),
        }
    }

    fn name(&self) -> &str {
        "IS_EMPTY"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// IS_NUMERIC(value) - Check if value can be parsed as number
struct IsNumericFunction;
impl TransformFunction for IsNumericFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        match &args[0] {
            Value::Integer(_) | Value::Float(_) | Value::Decimal(_) => Ok(Value::Boolean(true)),
            Value::String(s) => {
                let is_numeric = s.parse::<f64>().is_ok();
                Ok(Value::Boolean(is_numeric))
            }
            _ => Ok(Value::Boolean(false)),
        }
    }

    fn name(&self) -> &str {
        "IS_NUMERIC"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// IS_EMAIL(value) - Check if value looks like an email
struct IsEmailFunction;
impl TransformFunction for IsEmailFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Boolean(false));
        }

        let text = args[0].as_string();
        // Simple email regex
        let email_pattern = r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$";
        let re = Regex::new(email_pattern).unwrap();
        Ok(Value::Boolean(re.is_match(&text)))
    }

    fn name(&self) -> &str {
        "IS_EMAIL"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// IS_URL(value) - Check if value looks like a URL
struct IsUrlFunction;
impl TransformFunction for IsUrlFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Boolean(false));
        }

        let text = args[0].as_string();
        // Simple URL regex
        let url_pattern = r"^https?://[^\s/$.?#].[^\s]*$";
        let re = Regex::new(url_pattern).unwrap();
        Ok(Value::Boolean(re.is_match(&text)))
    }

    fn name(&self) -> &str {
        "IS_URL"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// AND(condition1, condition2, ...) - Logical AND
struct AndFunction;
impl TransformFunction for AndFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        for arg in args {
            if !arg.as_boolean() {
                return Ok(Value::Boolean(false));
            }
        }
        Ok(Value::Boolean(true))
    }

    fn name(&self) -> &str {
        "AND"
    }

    fn min_args(&self) -> usize {
        2
    }
}

/// OR(condition1, condition2, ...) - Logical OR
struct OrFunction;
impl TransformFunction for OrFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        for arg in args {
            if arg.as_boolean() {
                return Ok(Value::Boolean(true));
            }
        }
        Ok(Value::Boolean(false))
    }

    fn name(&self) -> &str {
        "OR"
    }

    fn min_args(&self) -> usize {
        2
    }
}

/// NOT(condition) - Logical NOT
struct NotFunction;
impl TransformFunction for NotFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        Ok(Value::Boolean(!args[0].as_boolean()))
    }

    fn name(&self) -> &str {
        "NOT"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

// Enhanced String functions

/// CAPITALIZE(text) - Capitalize first letter
struct CapitalizeFunction;
impl TransformFunction for CapitalizeFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let mut chars = text.chars();
        match chars.next() {
            None => Ok(Value::string_owned(String::new())),
            Some(first) => {
                let capitalized = first.to_uppercase().collect::<String>() + chars.as_str();
                Ok(Value::string_owned(capitalized))
            }
        }
    }

    fn name(&self) -> &str {
        "CAPITALIZE"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// TITLE_CASE(text) - Convert to title case
struct TitleCaseFunction;
impl TransformFunction for TitleCaseFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let result = text
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        Ok(Value::string_owned(result))
    }

    fn name(&self) -> &str {
        "TITLE_CASE"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// SLUG(text) - Convert to URL-safe slug
struct SlugFunction;
impl TransformFunction for SlugFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let slug = text
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' {
                    c
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");

        Ok(Value::string_owned(slug))
    }

    fn name(&self) -> &str {
        "SLUG"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// REMOVE_ACCENTS(text) - Remove diacritical marks
struct RemoveAccentsFunction;
impl TransformFunction for RemoveAccentsFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        // Basic accent removal (can be extended with unicode normalization)
        let result = text
            .chars()
            .map(|c| match c {
                'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => 'a',
                'è' | 'é' | 'ê' | 'ë' => 'e',
                'ì' | 'í' | 'î' | 'ï' => 'i',
                'ò' | 'ó' | 'ô' | 'õ' | 'ö' => 'o',
                'ù' | 'ú' | 'û' | 'ü' => 'u',
                'ñ' => 'n',
                'ç' => 'c',
                'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
                'È' | 'É' | 'Ê' | 'Ë' => 'E',
                'Ì' | 'Í' | 'Î' | 'Ï' => 'I',
                'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'O',
                'Ù' | 'Ú' | 'Û' | 'Ü' => 'U',
                'Ñ' => 'N',
                'Ç' => 'C',
                _ => c,
            })
            .collect();

        Ok(Value::string_owned(result))
    }

    fn name(&self) -> &str {
        "REMOVE_ACCENTS"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// TRANSLITERATE(text) - Convert to ASCII (alias for REMOVE_ACCENTS)
struct TransliterateFunction;
impl TransformFunction for TransliterateFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        // Delegate to REMOVE_ACCENTS for now
        RemoveAccentsFunction.execute(args)
    }

    fn name(&self) -> &str {
        "TRANSLITERATE"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(1)
    }
}

/// MASK(text, mask_char, visible_start, visible_end)
/// Mask sensitive data: MASK("1234567890", "*", 2, 2) => "12******90"
struct MaskFunction;
impl TransformFunction for MaskFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let mask_char = if args.len() > 1 && !args[1].is_null() {
            args[1].as_string().chars().next().unwrap_or('*')
        } else {
            '*'
        };
        let visible_start = if args.len() > 2 {
            args[2].as_integer()? as usize
        } else {
            0
        };
        let visible_end = if args.len() > 3 {
            args[3].as_integer()? as usize
        } else {
            0
        };

        let len = text.chars().count();
        let chars: Vec<char> = text.chars().collect();

        if visible_start + visible_end >= len {
            // If visible parts cover entire string, return as-is
            return Ok(Value::string_owned(text.into_owned()));
        }

        let mut result = String::new();
        for (i, c) in chars.iter().enumerate() {
            if i < visible_start || i >= len - visible_end {
                result.push(*c);
            } else {
                result.push(mask_char);
            }
        }

        Ok(Value::string_owned(result))
    }

    fn name(&self) -> &str {
        "MASK"
    }

    fn min_args(&self) -> usize {
        1
    }

    fn max_args(&self) -> Option<usize> {
        Some(4)
    }
}

/// TRUNCATE(text, length, suffix)
/// Truncate text to length: TRUNCATE("Hello World", 5, "...") => "Hello..."
struct TruncateFunction;
impl TransformFunction for TruncateFunction {
    fn execute(&self, args: &[Value]) -> Result<Value> {
        if args[0].is_null() {
            return Ok(Value::Null);
        }

        let text = args[0].as_string();
        let max_length = args[1].as_integer()? as usize;
        let suffix = if args.len() > 2 && !args[2].is_null() {
            args[2].as_string().into_owned()
        } else {
            String::from("...")
        };

        let len = text.chars().count();
        if len <= max_length {
            return Ok(Value::string_owned(text.into_owned()));
        }

        let truncated: String = text.chars().take(max_length).collect();
        Ok(Value::string_owned(truncated + &suffix))
    }

    fn name(&self) -> &str {
        "TRUNCATE"
    }

    fn min_args(&self) -> usize {
        2
    }

    fn max_args(&self) -> Option<usize> {
        Some(3)
    }
}
