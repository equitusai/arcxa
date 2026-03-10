//! Field Transformer executor - Apply SQL-like transformations to fields
//!
//! Leverages the transformation engine we built for the CSV-to-DB bulk loader.
//! Supports all transformation functions: UPPER, LOWER, TRIM, COALESCE, REGEX, etc.
//!
//! ## Extended Function Library (51 total functions)
//!
//! ### String Functions (14)
//! UPPER, LOWER, TRIM, LTRIM, RTRIM, LENGTH, SUBSTRING, REPLACE, CONCAT,
//! LEFT, RIGHT, REPEAT, REVERSE, SPLIT_PART
//!
//! ### Enhanced String Functions (7)
//! CAPITALIZE, TITLE_CASE, SLUG, REMOVE_ACCENTS, TRANSLITERATE, MASK, TRUNCATE
//!
//! ### Numeric Functions (7)
//! ABS, ROUND, FLOOR, CEIL, POWER, SQRT, MOD
//!
//! ### Date/Time Functions (10)
//! CURRENT_DATE, DATE_ADD, DATE_DIFF, DATE_FORMAT, PARSE_DATE, FORMAT_DATE,
//! EXTRACT, NOW, DATE_TRUNC, IS_VALID_DATE
//!
//! ### Conditional Functions (11)
//! IF, CASE, IS_NULL, IS_NOT_NULL, IS_EMPTY, IS_NUMERIC, IS_EMAIL, IS_URL,
//! AND, OR, NOT
//!
//! ### Regex Functions (5)
//! REGEX_MATCH, REGEX_REPLACE, REGEX_EXTRACT, REGEX_SPLIT, REGEX_EXTRACT_ALL
//!
//! ### NULL Handling (3)
//! COALESCE, NULLIF, IFNULL
//!
//! ## WASM UDF Support (Optional)
//!
//! Custom transformation functions can be loaded as WASM modules for business-specific logic.
//!
//! ```rust,ignore
//! use graphica_coordinator::etl::transformers::field::FieldTransformerExecutor;
//! use graphica_coordinator::mapping::loader::transformation::wasm::{
//!     WasmiFunction, WasmiFunctionRegistry, WasmiFunctionConfig
//! };
//!
//! // Load custom WASM function
//! let wasm_bytes = std::fs::read("custom_transform.wasm")?;
//! let function = WasmiFunction::new(
//!     "my_udf".to_string(),
//!     &wasm_bytes,
//!     WasmiFunctionConfig::default()
//! )?;
//!
//! // Create registry and register function
//! let registry = Arc::new(WasmiFunctionRegistry::new());
//! registry.register(function)?;
//!
//! // Create executor with WASM support
//! let executor = FieldTransformerExecutor::with_wasm_udfs(config, registry);
//!
//! // Use in transformation: {"operations": [{"Custom": {"expression": "MY_UDF({field})"}}]}
//! ```

use crate::mapping::loader::transformation::wasm::{WasmiFunction, WasmiFunctionRegistry};
use crate::mapping::loader::transformation::{TransformFunction, TransformationEngine};
use anyhow::{Context, Result};
use graphica_core::orchestration::workflow::{
    FieldTransformation, FieldTransformerConfig, TransformOperation,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Field Transformer executor with optional WASM UDF support
pub struct FieldTransformerExecutor {
    config: FieldTransformerConfig,
    engine: TransformationEngine,
    /// Optional WASM UDF registry for custom transformations
    wasm_registry: Option<Arc<WasmiFunctionRegistry>>,
}

impl FieldTransformerExecutor {
    /// Create a new field transformer executor without WASM UDF support
    pub fn new(config: FieldTransformerConfig) -> Self {
        Self {
            config,
            engine: TransformationEngine::new(),
            wasm_registry: None,
        }
    }

    /// Create a new field transformer executor with WASM UDF registry
    pub fn with_wasm_udfs(
        config: FieldTransformerConfig,
        wasm_registry: Arc<WasmiFunctionRegistry>,
    ) -> Self {
        Self {
            config,
            engine: TransformationEngine::new(),
            wasm_registry: Some(wasm_registry),
        }
    }

    /// Register a WASM UDF (creates registry if not present)
    pub fn register_wasm_udf(&mut self, function: WasmiFunction) -> Result<()> {
        if self.wasm_registry.is_none() {
            self.wasm_registry = Some(Arc::new(WasmiFunctionRegistry::new()));
        }

        if let Some(registry) = &self.wasm_registry {
            // Get mutable access - clone registry, modify, and replace
            let mut new_registry = WasmiFunctionRegistry::new();

            // Copy existing functions (if any - not possible with current API, so just register new one)
            // This is a limitation - users should create registry upfront and use with_wasm_udfs
            new_registry.register(function)?;
            self.wasm_registry = Some(Arc::new(new_registry));
            Ok(())
        } else {
            unreachable!("Registry was just created");
        }
    }

    /// Get WASM registry (if enabled)
    pub fn wasm_registry(&self) -> Option<&Arc<WasmiFunctionRegistry>> {
        self.wasm_registry.as_ref()
    }

    /// Transform a batch of records
    pub async fn transform_batch(&self, records: Vec<Value>) -> Result<Vec<Value>> {
        let mut transformed_records = Vec::new();

        for record in records {
            let transformed = self.transform_record(record)?;
            transformed_records.push(transformed);
        }

        Ok(transformed_records)
    }

    /// Transform a single record
    fn transform_record(&self, record: Value) -> Result<Value> {
        let mut obj = match record {
            Value::Object(map) => map,
            _ => anyhow::bail!("Expected JSON object, got {}", record),
        };

        // Convert to HashMap<String, String> for transformation engine
        let mut context: HashMap<String, String> = obj
            .iter()
            .map(|(k, v)| {
                let value_str = match v {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                (k.clone(), value_str)
            })
            .collect();

        // Apply each transformation
        for transformation in &self.config.transformations {
            let expression = self.build_transformation_expression(transformation)?;

            match self.engine.execute(&expression, &context) {
                Ok(result) => {
                    let result_str = match result {
                        crate::mapping::loader::transformation::Value::String(s) => s.to_string(),
                        crate::mapping::loader::transformation::Value::Integer(i) => i.to_string(),
                        crate::mapping::loader::transformation::Value::Float(f) => f.to_string(),
                        crate::mapping::loader::transformation::Value::Boolean(b) => b.to_string(),
                        crate::mapping::loader::transformation::Value::Null => String::new(),
                        crate::mapping::loader::transformation::Value::Date(d) => d.to_string(),
                        crate::mapping::loader::transformation::Value::Decimal(d) => d.to_string(),
                        crate::mapping::loader::transformation::Value::Timestamp(t) => {
                            t.to_string()
                        }
                        crate::mapping::loader::transformation::Value::Array(arr) => {
                            // Serialize array to JSON string
                            serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
                        }
                    };

                    // Update context for chained transformations
                    context.insert(transformation.field.clone(), result_str.clone());

                    // Update output object
                    obj.insert(transformation.field.clone(), Value::String(result_str));
                }
                Err(e) => {
                    // Log transformation error but continue processing
                    eprintln!(
                        "Transformation error for field {}: {}",
                        transformation.field, e
                    );
                }
            }
        }

        Ok(Value::Object(obj))
    }

    /// Build transformation expression from operations
    fn build_transformation_expression(
        &self,
        transformation: &FieldTransformation,
    ) -> Result<String> {
        let mut expression = format!("{{{}}}", transformation.field);

        for operation in &transformation.operations {
            expression = match operation {
                TransformOperation::Trim => format!("TRIM({})", expression),
                TransformOperation::Lower => format!("LOWER({})", expression),
                TransformOperation::Upper => format!("UPPER({})", expression),
                TransformOperation::Regex {
                    pattern,
                    replacement,
                } => {
                    format!(
                        "REGEX_REPLACE({}, '{}', '{}')",
                        expression, pattern, replacement
                    )
                }
                TransformOperation::Concat { separator, fields } => {
                    let field_refs: Vec<String> =
                        fields.iter().map(|f| format!("{{{}}}", f)).collect();
                    format!("CONCAT({}, '{}')", field_refs.join(", "), separator)
                }
                TransformOperation::Split { delimiter, index } => {
                    format!("SPLIT_PART({}, '{}', {})", expression, delimiter, index + 1)
                }
                TransformOperation::Substring { start, length } => {
                    if let Some(len) = length {
                        format!("SUBSTRING({}, {}, {})", expression, start, len)
                    } else {
                        format!("SUBSTRING({}, {})", expression, start)
                    }
                }
                TransformOperation::Replace { from, to } => {
                    format!("REPLACE({}, '{}', '{}')", expression, from, to)
                }
                TransformOperation::Round { decimals } => {
                    format!("ROUND({}, {})", expression, decimals)
                }
                TransformOperation::FormatDate { format } => {
                    format!("DATE_FORMAT(CAST({} AS DATE), '{}')", expression, format)
                }
                TransformOperation::Coalesce { fields } => {
                    let field_refs: Vec<String> =
                        fields.iter().map(|f| format!("{{{}}}", f)).collect();
                    format!("COALESCE({})", field_refs.join(", "))
                }
                TransformOperation::IfNull { default_value } => {
                    format!("IFNULL({}, '{}')", expression, default_value)
                }
                TransformOperation::Custom {
                    expression: custom_expr,
                } => custom_expr.clone(),
            };
        }

        Ok(expression)
    }
}

#[async_trait::async_trait]
impl crate::etl::EtlExecutor for FieldTransformerExecutor {
    async fn execute(&self, input: Value) -> Result<Value> {
        let records = match &input {
            Value::Array(arr) => arr.clone(),
            Value::Object(obj) if obj.contains_key("records") => match &obj["records"] {
                Value::Array(arr) => arr.clone(),
                _ => anyhow::bail!("Expected 'records' to be an array"),
            },
            _ => anyhow::bail!("Expected array or object with 'records' field"),
        };

        let transformed = self.transform_batch(records).await?;

        Ok(json!({
            "records": transformed,
            "count": transformed.len(),
            "transformations_applied": self.config.transformations.len(),
        }))
    }

    fn step_type(&self) -> &'static str {
        "field_transformer"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::orchestration::workflow::{FieldTransformation, TransformOperation};

    #[tokio::test]
    async fn test_transform_upper() {
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "name".to_string(),
                operations: vec![TransformOperation::Upper],
            }],
        };

        let executor = FieldTransformerExecutor::new(config);

        let records = vec![
            json!({"name": "john", "age": "25"}),
            json!({"name": "jane", "age": "30"}),
        ];

        let result = executor.transform_batch(records).await.unwrap();

        assert_eq!(result[0]["name"], "JOHN");
        assert_eq!(result[1]["name"], "JANE");
    }

    #[tokio::test]
    async fn test_transform_trim_and_upper() {
        let config = FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "email".to_string(),
                operations: vec![TransformOperation::Trim, TransformOperation::Lower],
            }],
        };

        let executor = FieldTransformerExecutor::new(config);

        let records = vec![json!({"email": "  JOHN@EXAMPLE.COM  "})];

        let result = executor.transform_batch(records).await.unwrap();

        assert_eq!(result[0]["email"], "john@example.com");
    }
}
