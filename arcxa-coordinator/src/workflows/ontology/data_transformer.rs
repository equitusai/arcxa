//! Data Transformation
//!
//! Transforms JSON data according to table schemas with robust type coercion,
//! case-insensitive field matching, and comprehensive validation.

use anyhow::{anyhow, Context, Result};
use serde_json::{Map, Value as JsonValue};
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

use super::types::{ColumnDefinition, TableSchema};

/// Trait for transforming JSON data to match table schemas
pub trait DataTransformer: Send + Sync {
    /// Validate a single row against the schema
    fn validate_row(&self, row: &Map<String, JsonValue>, schema: &TableSchema) -> Result<()>;

    /// Transform a single row to match the schema
    fn transform_row(
        &self,
        row: &Map<String, JsonValue>,
        schema: &TableSchema,
    ) -> Result<Map<String, JsonValue>>;

    /// Transform a batch of rows to match the schema
    fn transform_batch(
        &self,
        rows: &[Map<String, JsonValue>],
        schema: &TableSchema,
    ) -> Result<Vec<Map<String, JsonValue>>>;
}

/// Default implementation of DataTransformer with comprehensive type coercion
#[derive(Debug)]
pub struct DefaultDataTransformer {
    /// Whether to preserve unmapped fields (default: false)
    preserve_unmapped: bool,
    /// Whether to generate auto-increment IDs (default: false)
    auto_generate_ids: bool,
    /// Starting value for auto-increment IDs
    id_counter: Arc<AtomicI64>,
}

impl DefaultDataTransformer {
    /// Create a new default data transformer
    pub fn new() -> Self {
        Self {
            preserve_unmapped: false,
            auto_generate_ids: false,
            id_counter: Arc::new(AtomicI64::new(1)),
        }
    }

    /// Set whether to preserve unmapped fields
    pub fn with_preserve_unmapped(mut self, preserve: bool) -> Self {
        self.preserve_unmapped = preserve;
        self
    }

    /// Set whether to auto-generate IDs for primary key columns
    pub fn with_auto_generate_ids(mut self, generate: bool) -> Self {
        self.auto_generate_ids = generate;
        self
    }

    /// Build a case-insensitive mapping from JSON keys to schema columns
    fn build_field_mapping(
        &self,
        row: &Map<String, JsonValue>,
        schema: &TableSchema,
    ) -> HashMap<String, String> {
        let mut mapping = HashMap::new();

        // Build normalized column name lookup (lowercase)
        let mut normalized_columns: HashMap<String, String> = HashMap::new();
        for col in &schema.columns {
            normalized_columns.insert(col.name.to_lowercase(), col.name.clone());
        }

        // Map each row key to a schema column (case-insensitive)
        for key in row.keys() {
            let normalized_key = key.to_lowercase();
            if let Some(col_name) = normalized_columns.get(&normalized_key) {
                mapping.insert(key.clone(), col_name.clone());
            }
        }

        mapping
    }

    /// Convert JSON value to match column SQL type with comprehensive coercion
    fn coerce_value(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        // Handle null values
        if value.is_null() {
            if column.nullable {
                return Ok(JsonValue::Null);
            } else {
                return Err(anyhow!(
                    "Column '{}' is not nullable but received null value",
                    column.name
                ));
            }
        }

        // Parse SQL type (handle types like VARCHAR(255), DECIMAL(19,4))
        let base_type = column
            .sql_type
            .split('(')
            .next()
            .unwrap_or(&column.sql_type)
            .to_uppercase();

        match base_type.as_str() {
            // String types
            "VARCHAR" | "CHAR" | "TEXT" | "CLOB" => self.coerce_to_string(value, column),

            // Integer types
            "INTEGER" | "INT" | "BIGINT" | "SMALLINT" | "TINYINT" => {
                self.coerce_to_integer(value, column)
            }

            // Decimal types
            "DECIMAL" | "NUMERIC" | "REAL" | "DOUBLE" | "FLOAT" => {
                self.coerce_to_decimal(value, column)
            }

            // Boolean types
            "BOOLEAN" | "BOOL" => self.coerce_to_boolean(value, column),

            // Date/Time types
            "DATE" => self.coerce_to_date(value, column),
            "TIME" => self.coerce_to_time(value, column),
            "TIMESTAMP" | "DATETIME" => self.coerce_to_timestamp(value, column),

            // Binary types
            "BLOB" | "BYTEA" | "BINARY" | "VARBINARY" => self.coerce_to_binary(value, column),

            // JSON types
            "JSON" | "JSONB" => Ok(value.clone()),

            // Unknown types - pass through as string
            _ => {
                warn!(
                    "Unknown SQL type '{}' for column '{}', converting to string",
                    column.sql_type, column.name
                );
                self.coerce_to_string(value, column)
            }
        }
    }

    /// Coerce value to string with length validation
    fn coerce_to_string(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let string_value = match value {
            JsonValue::String(s) => s.clone(),
            JsonValue::Number(n) => n.to_string(),
            JsonValue::Bool(b) => b.to_string(),
            JsonValue::Array(_) | JsonValue::Object(_) => serde_json::to_string(value)?,
            JsonValue::Null => {
                return Err(anyhow!(
                    "Cannot coerce null to string for column '{}'",
                    column.name
                ));
            }
        };

        // Check max length if specified in SQL type (e.g., VARCHAR(255))
        if let Some(max_len) = self.extract_varchar_length(&column.sql_type) {
            if string_value.len() > max_len {
                return Err(anyhow!(
                    "String value for column '{}' exceeds max length {} (actual: {})",
                    column.name,
                    max_len,
                    string_value.len()
                ));
            }
        }

        Ok(JsonValue::String(string_value))
    }

    /// Coerce value to integer with validation
    fn coerce_to_integer(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let int_value = match value {
            JsonValue::Number(n) => n.as_i64().ok_or_else(|| {
                anyhow!(
                    "Number is out of range for integer column '{}'",
                    column.name
                )
            })?,
            JsonValue::String(s) => s.trim().parse::<i64>().context(format!(
                "Failed to parse string '{}' as integer for column '{}'",
                s, column.name
            ))?,
            JsonValue::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to integer for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        Ok(JsonValue::Number(int_value.into()))
    }

    /// Coerce value to decimal with precision/scale validation
    fn coerce_to_decimal(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let float_value = match value {
            JsonValue::Number(n) => n.as_f64().ok_or_else(|| {
                anyhow!(
                    "Number is out of range for decimal column '{}'",
                    column.name
                )
            })?,
            JsonValue::String(s) => s.trim().parse::<f64>().context(format!(
                "Failed to parse string '{}' as decimal for column '{}'",
                s, column.name
            ))?,
            JsonValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to decimal for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        // Validate precision/scale if specified
        if let Some((precision, scale)) = self.extract_decimal_precision(&column.sql_type) {
            // Check total digits
            let abs_value = float_value.abs();
            let total_digits = if abs_value == 0.0 {
                1
            } else {
                (abs_value.log10().floor() as i32) + 1 + scale as i32
            };

            if total_digits > precision as i32 {
                return Err(anyhow!(
                    "Decimal value for column '{}' exceeds precision {} (value: {})",
                    column.name,
                    precision,
                    float_value
                ));
            }
        }

        Ok(serde_json::Number::from_f64(float_value)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(float_value.to_string())))
    }

    /// Coerce value to boolean with flexible string parsing
    fn coerce_to_boolean(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let bool_value = match value {
            JsonValue::Bool(b) => *b,
            JsonValue::Number(n) => n.as_i64().unwrap_or(0) != 0,
            JsonValue::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "true" | "t" | "yes" | "y" | "1" => true,
                    "false" | "f" | "no" | "n" | "0" => false,
                    _ => {
                        return Err(anyhow!(
                            "Cannot parse string '{}' as boolean for column '{}'",
                            s,
                            column.name
                        ));
                    }
                }
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to boolean for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        Ok(JsonValue::Bool(bool_value))
    }

    /// Coerce value to date (ISO 8601 format: YYYY-MM-DD)
    fn coerce_to_date(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let date_str = match value {
            JsonValue::String(s) => {
                // Validate ISO 8601 date format
                chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                    .or_else(|_| chrono::NaiveDate::parse_from_str(s.trim(), "%Y/%m/%d"))
                    .context(format!(
                        "Failed to parse date '{}' for column '{}' (expected YYYY-MM-DD)",
                        s, column.name
                    ))?;
                s.clone()
            }
            JsonValue::Number(n) => {
                // Assume Unix timestamp (seconds)
                let timestamp = n.as_i64().ok_or_else(|| {
                    anyhow!("Invalid timestamp for date column '{}'", column.name)
                })?;
                let naive =
                    chrono::NaiveDateTime::from_timestamp_opt(timestamp, 0).ok_or_else(|| {
                        anyhow!("Timestamp out of range for column '{}'", column.name)
                    })?;
                naive.format("%Y-%m-%d").to_string()
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to date for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        Ok(JsonValue::String(date_str))
    }

    /// Coerce value to time (ISO 8601 format: HH:MM:SS)
    fn coerce_to_time(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        let time_str = match value {
            JsonValue::String(s) => {
                // Validate ISO 8601 time format
                chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M:%S")
                    .or_else(|_| chrono::NaiveTime::parse_from_str(s.trim(), "%H:%M"))
                    .context(format!(
                        "Failed to parse time '{}' for column '{}' (expected HH:MM:SS)",
                        s, column.name
                    ))?;
                s.clone()
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to time for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        Ok(JsonValue::String(time_str))
    }

    /// Coerce value to timestamp (ISO 8601 format: YYYY-MM-DDTHH:MM:SS)
    fn coerce_to_timestamp(
        &self,
        value: &JsonValue,
        column: &ColumnDefinition,
    ) -> Result<JsonValue> {
        let timestamp_str = match value {
            JsonValue::String(s) => {
                // Validate ISO 8601 timestamp format
                chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%dT%H:%M:%S")
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S")
                    })
                    .or_else(|_| {
                        // Try parsing RFC3339 (with timezone)
                        chrono::DateTime::parse_from_rfc3339(s.trim()).map(|dt| dt.naive_utc())
                    })
                    .context(format!(
                        "Failed to parse timestamp '{}' for column '{}' (expected ISO 8601)",
                        s, column.name
                    ))?;
                s.clone()
            }
            JsonValue::Number(n) => {
                // Assume Unix timestamp (seconds)
                let timestamp = n
                    .as_i64()
                    .ok_or_else(|| anyhow!("Invalid timestamp for column '{}'", column.name))?;
                let naive =
                    chrono::NaiveDateTime::from_timestamp_opt(timestamp, 0).ok_or_else(|| {
                        anyhow!("Timestamp out of range for column '{}'", column.name)
                    })?;
                naive.format("%Y-%m-%dT%H:%M:%S").to_string()
            }
            _ => {
                return Err(anyhow!(
                    "Cannot coerce {} to timestamp for column '{}'",
                    value,
                    column.name
                ));
            }
        };

        Ok(JsonValue::String(timestamp_str))
    }

    /// Coerce value to binary (base64 encoded)
    fn coerce_to_binary(&self, value: &JsonValue, column: &ColumnDefinition) -> Result<JsonValue> {
        match value {
            JsonValue::String(s) => {
                // Validate base64
                base64::decode(s.trim()).context(format!(
                    "Failed to decode base64 for binary column '{}' (invalid base64)",
                    column.name
                ))?;
                Ok(JsonValue::String(s.clone()))
            }
            _ => Err(anyhow!(
                "Cannot coerce {} to binary for column '{}' (expected base64 string)",
                value,
                column.name
            )),
        }
    }

    /// Extract VARCHAR max length from SQL type (e.g., VARCHAR(255) -> Some(255))
    fn extract_varchar_length(&self, sql_type: &str) -> Option<usize> {
        if let Some(start) = sql_type.find('(') {
            if let Some(end) = sql_type.find(')') {
                if let Ok(len) = sql_type[start + 1..end].trim().parse::<usize>() {
                    return Some(len);
                }
            }
        }
        None
    }

    /// Extract DECIMAL precision and scale (e.g., DECIMAL(19,4) -> Some((19, 4)))
    fn extract_decimal_precision(&self, sql_type: &str) -> Option<(u32, u32)> {
        if let Some(start) = sql_type.find('(') {
            if let Some(end) = sql_type.find(')') {
                let parts: Vec<&str> = sql_type[start + 1..end].split(',').collect();
                if parts.len() == 2 {
                    if let (Ok(precision), Ok(scale)) = (
                        parts[0].trim().parse::<u32>(),
                        parts[1].trim().parse::<u32>(),
                    ) {
                        return Some((precision, scale));
                    }
                }
            }
        }
        None
    }

    /// Generate next auto-increment ID
    fn next_id(&self) -> i64 {
        self.id_counter.fetch_add(1, Ordering::SeqCst)
    }
}

impl Default for DefaultDataTransformer {
    fn default() -> Self {
        Self::new()
    }
}

impl DataTransformer for DefaultDataTransformer {
    fn validate_row(&self, row: &Map<String, JsonValue>, schema: &TableSchema) -> Result<()> {
        // Build field mapping (case-insensitive)
        let field_mapping = self.build_field_mapping(row, schema);

        // Check that all required columns are present
        for column in &schema.columns {
            if !column.nullable && !column.is_primary_key {
                // Required column must have a corresponding field
                let has_field = field_mapping
                    .iter()
                    .any(|(_, target)| target == &column.name);

                if !has_field {
                    return Err(anyhow!(
                        "Required column '{}' has no corresponding field in row",
                        column.name
                    ));
                }
            }
        }

        Ok(())
    }

    fn transform_row(
        &self,
        row: &Map<String, JsonValue>,
        schema: &TableSchema,
    ) -> Result<Map<String, JsonValue>> {
        let mut result = Map::new();

        // Build field mapping (case-insensitive)
        let field_mapping = self.build_field_mapping(row, schema);

        // Process each column in the schema
        for column in &schema.columns {
            // Check if column is auto-generated
            if self.auto_generate_ids
                && column.is_primary_key
                && !field_mapping.values().any(|v| v == &column.name)
            {
                // Auto-generate ID
                let id = self.next_id();
                result.insert(column.name.clone(), JsonValue::Number(id.into()));
                debug!("Auto-generated ID {} for column '{}'", id, column.name);
                continue;
            }

            // Find source field for this column
            let source_field = field_mapping
                .iter()
                .find(|(_, target)| *target == &column.name)
                .map(|(source, _)| source);

            if let Some(source) = source_field {
                // Get value from row
                if let Some(value) = row.get(source) {
                    // Coerce value to match column type
                    match self.coerce_value(value, column) {
                        Ok(coerced) => {
                            result.insert(column.name.clone(), coerced);
                        }
                        Err(e) => {
                            return Err(anyhow!(
                                "Failed to transform field '{}' -> '{}': {}",
                                source,
                                column.name,
                                e
                            ));
                        }
                    }
                } else if !column.nullable {
                    return Err(anyhow!(
                        "Required column '{}' is missing in row",
                        column.name
                    ));
                } else {
                    // Column is nullable and not present
                    result.insert(column.name.clone(), JsonValue::Null);
                }
            } else if !column.nullable && !column.is_primary_key {
                return Err(anyhow!(
                    "Required column '{}' has no corresponding field in row",
                    column.name
                ));
            } else {
                // Column is nullable and not mapped
                result.insert(column.name.clone(), JsonValue::Null);
            }
        }

        // Optionally preserve unmapped fields
        if self.preserve_unmapped {
            for (key, value) in row.iter() {
                if !field_mapping.contains_key(key) {
                    result.insert(key.clone(), value.clone());
                }
            }
        }

        Ok(result)
    }

    fn transform_batch(
        &self,
        rows: &[Map<String, JsonValue>],
        schema: &TableSchema,
    ) -> Result<Vec<Map<String, JsonValue>>> {
        let mut results = Vec::with_capacity(rows.len());

        for (idx, row) in rows.iter().enumerate() {
            match self.transform_row(row, schema) {
                Ok(transformed) => results.push(transformed),
                Err(e) => {
                    return Err(anyhow!(
                        "Failed to transform row {} of {}: {}",
                        idx + 1,
                        rows.len(),
                        e
                    ));
                }
            }
        }

        info!("Successfully transformed {} rows", results.len());
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_schema() -> TableSchema {
        let mut schema = TableSchema::new("TEST_TABLE".to_string());

        schema.add_column(
            ColumnDefinition::new("id".to_string(), "INTEGER".to_string(), false).as_primary_key(),
        );
        schema.add_column(ColumnDefinition::new(
            "name".to_string(),
            "VARCHAR(255)".to_string(),
            false,
        ));
        schema.add_column(ColumnDefinition::new(
            "email".to_string(),
            "VARCHAR(255)".to_string(),
            true,
        ));
        schema.add_column(ColumnDefinition::new(
            "age".to_string(),
            "INTEGER".to_string(),
            true,
        ));
        schema.add_column(ColumnDefinition::new(
            "balance".to_string(),
            "DECIMAL(19,4)".to_string(),
            true,
        ));
        schema.add_column(ColumnDefinition::new(
            "active".to_string(),
            "BOOLEAN".to_string(),
            true,
        ));
        schema.add_column(ColumnDefinition::new(
            "created_at".to_string(),
            "TIMESTAMP".to_string(),
            true,
        ));

        schema
    }

    #[test]
    fn test_transform_row_basic() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert(
            "name".to_string(),
            JsonValue::String("John Doe".to_string()),
        );
        row.insert(
            "email".to_string(),
            JsonValue::String("john@example.com".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(result.get("id"), Some(&JsonValue::Number(1.into())));
        assert_eq!(
            result.get("name"),
            Some(&JsonValue::String("John Doe".to_string()))
        );
        assert_eq!(
            result.get("email"),
            Some(&JsonValue::String("john@example.com".to_string()))
        );
    }

    #[test]
    fn test_transform_row_case_insensitive() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("ID".to_string(), JsonValue::Number(1.into()));
        row.insert("NAME".to_string(), JsonValue::String("John".to_string()));
        row.insert(
            "EmAiL".to_string(),
            JsonValue::String("john@example.com".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(result.get("id"), Some(&JsonValue::Number(1.into())));
        assert_eq!(
            result.get("name"),
            Some(&JsonValue::String("John".to_string()))
        );
    }

    #[test]
    fn test_coerce_number_to_string() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::Number(12345.into())); // number as name
        row.insert(
            "email".to_string(),
            JsonValue::String("test@example.com".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(
            result.get("name"),
            Some(&JsonValue::String("12345".to_string()))
        );
    }

    #[test]
    fn test_coerce_string_to_number() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::String("42".to_string()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert("age".to_string(), JsonValue::String("30".to_string()));

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(result.get("id"), Some(&JsonValue::Number(42.into())));
        assert_eq!(result.get("age"), Some(&JsonValue::Number(30.into())));
    }

    #[test]
    fn test_coerce_boolean_variations() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let test_cases = vec![
            ("true", true),
            ("TRUE", true),
            ("t", true),
            ("yes", true),
            ("1", true),
            ("false", false),
            ("FALSE", false),
            ("f", false),
            ("no", false),
            ("0", false),
        ];

        for (input, expected) in test_cases {
            let mut row = Map::new();
            row.insert("id".to_string(), JsonValue::Number(1.into()));
            row.insert("name".to_string(), JsonValue::String("Test".to_string()));
            row.insert("active".to_string(), JsonValue::String(input.to_string()));

            let result = transformer.transform_row(&row, &schema).unwrap();
            assert_eq!(
                result.get("active"),
                Some(&JsonValue::Bool(expected)),
                "Failed for input: {}",
                input
            );
        }
    }

    #[test]
    fn test_coerce_date() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert(
            "created_at".to_string(),
            JsonValue::String("2024-01-15T10:30:00".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert!(result.get("created_at").unwrap().is_string());
    }

    #[test]
    fn test_nullable_columns() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        // email is nullable and not provided

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(result.get("email"), Some(&JsonValue::Null));
    }

    #[test]
    fn test_required_column_missing() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        // name is required but missing

        let result = transformer.transform_row(&row, &schema);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Required column"));
    }

    #[test]
    fn test_auto_generate_ids() {
        let transformer = DefaultDataTransformer::new().with_auto_generate_ids(true);
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        // id not provided

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert!(result.get("id").is_some());
        assert!(result.get("id").unwrap().is_number());
    }

    #[test]
    fn test_preserve_unmapped() {
        let transformer = DefaultDataTransformer::new().with_preserve_unmapped(true);
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert(
            "extra_field".to_string(),
            JsonValue::String("extra_value".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert_eq!(
            result.get("extra_field"),
            Some(&JsonValue::String("extra_value".to_string()))
        );
    }

    #[test]
    fn test_transform_batch() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut rows = Vec::new();
        for i in 1..=3 {
            let mut row = Map::new();
            row.insert("id".to_string(), JsonValue::Number(i.into()));
            row.insert("name".to_string(), JsonValue::String(format!("User {}", i)));
            rows.push(row);
        }

        let result = transformer.transform_batch(&rows, &schema).unwrap();

        assert_eq!(result.len(), 3);
        for (i, row) in result.iter().enumerate() {
            assert_eq!(
                row.get("id"),
                Some(&JsonValue::Number((i as i64 + 1).into()))
            );
        }
    }

    #[test]
    fn test_varchar_length_validation() {
        let transformer = DefaultDataTransformer::new();
        let mut schema = TableSchema::new("TEST".to_string());
        schema.add_column(ColumnDefinition::new(
            "short_text".to_string(),
            "VARCHAR(5)".to_string(),
            false,
        ));

        let mut row = Map::new();
        row.insert(
            "short_text".to_string(),
            JsonValue::String("toolong".to_string()),
        );

        let result = transformer.transform_row(&row, &schema);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds max length"));
    }

    #[test]
    fn test_invalid_integer_coercion() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert(
            "id".to_string(),
            JsonValue::String("not_a_number".to_string()),
        );
        row.insert("name".to_string(), JsonValue::String("John".to_string()));

        let result = transformer.transform_row(&row, &schema);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to parse"));
    }

    #[test]
    fn test_decimal_coercion() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert(
            "balance".to_string(),
            JsonValue::String("1234.56".to_string()),
        );

        let result = transformer.transform_row(&row, &schema).unwrap();

        assert!(
            result.get("balance").unwrap().is_number()
                || result.get("balance").unwrap().is_string()
        );
    }

    #[test]
    fn test_boolean_number_coercion() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert("active".to_string(), JsonValue::Number(1.into()));

        let result = transformer.transform_row(&row, &schema).unwrap();
        assert_eq!(result.get("active"), Some(&JsonValue::Bool(true)));

        let mut row2 = Map::new();
        row2.insert("id".to_string(), JsonValue::Number(2.into()));
        row2.insert("name".to_string(), JsonValue::String("Jane".to_string()));
        row2.insert("active".to_string(), JsonValue::Number(0.into()));

        let result2 = transformer.transform_row(&row2, &schema).unwrap();
        assert_eq!(result2.get("active"), Some(&JsonValue::Bool(false)));
    }

    #[test]
    fn test_timestamp_unix_coercion() {
        let transformer = DefaultDataTransformer::new();
        let schema = create_test_schema();

        let mut row = Map::new();
        row.insert("id".to_string(), JsonValue::Number(1.into()));
        row.insert("name".to_string(), JsonValue::String("John".to_string()));
        row.insert(
            "created_at".to_string(),
            JsonValue::Number(1609459200.into()),
        ); // 2021-01-01 00:00:00 UTC

        let result = transformer.transform_row(&row, &schema).unwrap();
        assert!(result.get("created_at").unwrap().is_string());
    }
}
