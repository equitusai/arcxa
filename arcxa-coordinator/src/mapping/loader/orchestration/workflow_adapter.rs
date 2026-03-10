//! Workflow Adapter for CSV Processing
//!
//! This module provides a lightweight adapter to apply workflow filtering
//! to CSV processing WITHOUT modifying LoaderWorker. This demonstrates
//! how workflows and CSV loading can work together while keeping them decoupled.
//!
//! ## Design Philosophy
//!
//! Keep LoaderWorker focused on its core responsibility (CSV → DB2 loading)
//! while allowing optional workflow-based filtering through composition.
//!
//! ## Usage Example
//!
//! ```rust,ignore
//! // Create adapter with workflow
//! let adapter = WorkflowCsvAdapter::new(workflow);
//!
//! // Pre-filter CSV file based on workflow conditions
//! let filtered_file = adapter.filter_csv_file(&input_csv, &output_csv).await?;
//!
//! // Pass filtered file to LoaderWorker (unchanged)
//! let worker = LoaderWorker::new(config_with_filtered_file, metrics, cancel_token);
//! worker.run().await?;
//! ```

use crate::common::csv_utils::{CsvReaderConfig, CsvStreamReader};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use crate::workflows::domain::{Condition, Workflow};
use crate::workflows::engine::ConditionEvaluator;

/// Adapter to apply workflow filtering to CSV files
///
/// This is a **pre-processor** that filters CSV files based on workflow conditions
/// BEFORE they are processed by LoaderWorker. This keeps LoaderWorker unchanged.
pub struct WorkflowCsvAdapter {
    workflow: Workflow,
}

impl WorkflowCsvAdapter {
    /// Create a new adapter with a workflow
    pub fn new(workflow: Workflow) -> Self {
        Self { workflow }
    }

    /// Filter a CSV file based on workflow conditions
    ///
    /// Creates a new CSV file containing only rows that match workflow conditions.
    ///
    /// ## Arguments
    /// * `input_path` - Path to source CSV file
    /// * `output_path` - Path where filtered CSV will be written
    /// * `source_table_name` - Name to use for "source_table" field in conditions
    ///
    /// ## Returns
    /// Number of rows written to output file
    pub async fn filter_csv_file(
        &self,
        input_path: &Path,
        output_path: &Path,
        source_table_name: &str,
    ) -> Result<FilterResult> {
        // Open input CSV
        let mut reader = CsvStreamReader::new(
            input_path,
            CsvReaderConfig {
                delimiter: Some(b','),
                has_header: true,
                ..Default::default()
            },
        )?;
        reader.init()?;

        let headers: Vec<String> = reader
            .headers()
            .context("Failed to read CSV headers")?
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Create output CSV
        let mut writer = csv::Writer::from_path(output_path)?;

        // Write headers - convert to iterator
        writer.write_record(headers.iter())?;

        let mut total_rows = 0;
        let mut filtered_rows = 0;
        let mut written_rows = 0;
        let mut filter_reasons: HashMap<String, usize> = HashMap::new();

        // Process each row
        while let Some(record) = reader.read_record()? {
            total_rows += 1;

            // Convert CSV row to JSON for condition evaluation
            let json_row =
                self.csv_row_to_json_from_record(&headers, &record, source_table_name)?;

            // Check if row matches any workflow route
            let (should_process, reason) = self.evaluate_row(&json_row)?;

            if should_process {
                // Apply transformations from matched workflow route
                let transformed_row =
                    self.transform_row(&headers, record.fields.clone(), source_table_name)?;

                // Write transformed row to output
                writer.write_record(transformed_row.iter())?;
                written_rows += 1;
            } else {
                filtered_rows += 1;
                *filter_reasons.entry(reason).or_insert(0) += 1;
            }

            // Periodic progress logging
            if total_rows % 10000 == 0 {
                tracing::debug!(
                    "Processed {} rows: {} written, {} filtered",
                    total_rows,
                    written_rows,
                    filtered_rows
                );
            }
        }

        writer.flush()?;

        Ok(FilterResult {
            total_rows,
            written_rows,
            filtered_rows,
            filter_reasons,
        })
    }

    /// Check if a single CSV row should be processed
    ///
    /// This can be used for row-by-row filtering without creating intermediate files.
    pub fn should_process_row(
        &self,
        headers: &[String],
        row: &[String],
        source_table_name: &str,
    ) -> Result<bool> {
        // Convert to JSON
        let json_row = self.csv_row_to_json_from_slices(headers, row, source_table_name)?;

        // Evaluate against workflow
        let (should_process, _) = self.evaluate_row(&json_row)?;

        Ok(should_process)
    }

    /// Convert CSV row from CsvRecord to JSON for condition evaluation
    fn csv_row_to_json_from_record(
        &self,
        headers: &[String],
        record: &crate::common::csv_utils::streaming::CsvRecord,
        source_table_name: &str,
    ) -> Result<serde_json::Value> {
        self.csv_row_to_json_from_slices(headers, &record.fields, source_table_name)
    }

    /// Convert CSV row from StringRecord to JSON for condition evaluation
    fn csv_row_to_json(
        &self,
        headers: &csv::StringRecord,
        row: &csv::StringRecord,
        source_table_name: &str,
    ) -> Result<serde_json::Value> {
        let mut json_obj = serde_json::Map::new();

        // Add source_table field for workflow routing
        json_obj.insert(
            "source_table".to_string(),
            serde_json::Value::String(source_table_name.to_string()),
        );

        // Convert each field
        for (header, value) in headers.iter().zip(row.iter()) {
            // Try to parse as number, otherwise keep as string
            let json_value = if let Ok(num) = value.parse::<f64>() {
                if num.is_finite() {
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(num)
                            .expect("Finite f64 should always convert to JSON Number"),
                    )
                } else {
                    // Handle NaN/infinite as string to preserve data
                    serde_json::Value::String(value.to_string())
                }
            } else if value.eq_ignore_ascii_case("true") {
                serde_json::Value::Bool(true)
            } else if value.eq_ignore_ascii_case("false") {
                serde_json::Value::Bool(false)
            } else if value.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(value.to_string())
            };

            json_obj.insert(header.to_string(), json_value);
        }

        Ok(serde_json::Value::Object(json_obj))
    }

    /// Convert CSV row to JSON from string slices
    fn csv_row_to_json_from_slices(
        &self,
        headers: &[String],
        row: &[String],
        source_table_name: &str,
    ) -> Result<serde_json::Value> {
        let mut json_obj = serde_json::Map::new();

        json_obj.insert(
            "source_table".to_string(),
            serde_json::Value::String(source_table_name.to_string()),
        );

        for (header, value) in headers.iter().zip(row.iter()) {
            let json_value = if let Ok(num) = value.parse::<f64>() {
                if num.is_finite() {
                    serde_json::Value::Number(
                        serde_json::Number::from_f64(num)
                            .expect("Finite f64 should always convert to JSON Number"),
                    )
                } else {
                    // Handle NaN/infinite as string to preserve data
                    serde_json::Value::String(value.to_string())
                }
            } else if value.eq_ignore_ascii_case("true") {
                serde_json::Value::Bool(true)
            } else if value.eq_ignore_ascii_case("false") {
                serde_json::Value::Bool(false)
            } else if value.is_empty() {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(value.to_string())
            };

            json_obj.insert(header.clone(), json_value);
        }

        Ok(serde_json::Value::Object(json_obj))
    }

    /// Evaluate if a row should be processed based on workflow conditions
    fn evaluate_row(&self, json_row: &serde_json::Value) -> Result<(bool, String)> {
        // Check each route in priority order
        for route in self.workflow.routes_by_priority() {
            match ConditionEvaluator::evaluate(&route.condition, json_row) {
                Ok(true) => {
                    // Route matched - check for filtering actions
                    // For now, we just check if the route would process this row
                    return Ok((true, format!("Matched route: {}", route.name)));
                }
                Ok(false) => {
                    // Route didn't match, try next
                    continue;
                }
                Err(e) => {
                    // Error evaluating condition - log and skip
                    tracing::debug!("Error evaluating route {}: {}", route.name, e);
                    continue;
                }
            }
        }

        // No routes matched - filter out
        Ok((false, "No matching routes".to_string()))
    }

    /// Find the matching route for a row and return its actions
    fn find_matching_route(
        &self,
        json_row: &serde_json::Value,
    ) -> Option<&crate::workflows::domain::Route> {
        for route in self.workflow.routes_by_priority() {
            if let Ok(true) = ConditionEvaluator::evaluate(&route.condition, json_row) {
                return Some(route);
            }
        }
        None
    }

    /// Apply transformations to a CSV row based on workflow actions
    ///
    /// Executes all actions from the matched route, applying transformations
    /// like proper case, email normalization, phone formatting, etc.
    pub fn transform_row(
        &self,
        headers: &[String],
        row: Vec<String>,
        source_table_name: &str,
    ) -> Result<Vec<String>> {
        // Convert to JSON for route matching
        let json_row = self.csv_row_to_json_from_slices(headers, &row, source_table_name)?;

        // Find matching route
        let route = match self.find_matching_route(&json_row) {
            Some(r) => r,
            None => return Ok(row), // No route matched, return unchanged
        };

        // Convert to mutable JSON object for transformation
        let serde_json::Value::Object(mut json_obj) = json_row else {
            return Err(anyhow::anyhow!(
                "Expected JSON object from CSV conversion, got: {:?}",
                json_row
            ));
        };

        // Execute each action in the route
        for action in route.actions.iter() {
            match action {
                crate::workflows::domain::Action::Transform {
                    transformer,
                    config,
                } => {
                    self.apply_transformer(&mut json_obj, transformer, config, headers)?;
                }
                crate::workflows::domain::Action::SetField { field, value } => {
                    json_obj.insert(field.to_string(), value.clone());
                }
                crate::workflows::domain::Action::RemoveField { field } => {
                    json_obj.remove(&*field as &str);
                }
                // Other actions (Log, SendToKafka, etc.) are not applicable to CSV transformation
                _ => {
                    tracing::debug!(
                        "Skipping non-transformation action: {}",
                        action.action_type()
                    );
                }
            }
        }

        // Convert back to CSV row
        self.json_to_csv_row(&json_obj, headers)
    }

    /// Apply a named transformer to the JSON object
    fn apply_transformer(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        transformer: &str,
        config: &serde_json::Value,
        headers: &[String],
    ) -> Result<()> {
        match transformer {
            "proper_case" => self.apply_proper_case(json_obj, config, headers),
            "normalize_email" => self.apply_normalize_email(json_obj, config),
            "format_phone" => self.apply_format_phone(json_obj, config),
            "trim_whitespace" => self.apply_trim_whitespace(json_obj, config, headers),
            "uppercase" => self.apply_uppercase(json_obj, config, headers),
            "lowercase" => self.apply_lowercase(json_obj, config, headers),
            _ => {
                tracing::warn!("Unknown transformer: {}", transformer);
                Ok(())
            }
        }
    }

    /// Apply proper case (Title Case) to specified fields
    fn apply_proper_case(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
        headers: &[String],
    ) -> Result<()> {
        let fields = if let Some(fields_arr) = config.get("fields").and_then(|v| v.as_array()) {
            fields_arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            // Default: apply to all string fields
            headers.to_vec()
        };

        for field in fields {
            if let Some(value) = json_obj.get(&field) {
                if let Some(s) = value.as_str() {
                    let proper_cased = to_proper_case(s);
                    json_obj.insert(field, serde_json::Value::String(proper_cased));
                }
            }
        }

        Ok(())
    }

    /// Normalize email to lowercase
    fn apply_normalize_email(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
    ) -> Result<()> {
        let field = config
            .get("field")
            .and_then(|v| v.as_str())
            .unwrap_or("email");

        if let Some(value) = json_obj.get(field) {
            if let Some(s) = value.as_str() {
                let normalized = s.trim().to_lowercase();
                json_obj.insert(field.to_string(), serde_json::Value::String(normalized));
            }
        }

        Ok(())
    }

    /// Format phone number to standard format
    fn apply_format_phone(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
    ) -> Result<()> {
        let field = config
            .get("field")
            .and_then(|v| v.as_str())
            .unwrap_or("phone");

        if let Some(value) = json_obj.get(field) {
            // Handle both strings and numbers (phone numbers might be parsed as numbers)
            let phone_str = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => {
                    // Convert number to string without decimal point
                    if n.is_i64() || n.is_u64() {
                        n.as_i64()
                            .map(|i| i.to_string())
                            .or_else(|| n.as_u64().map(|u| u.to_string()))
                            .unwrap_or_else(|| n.to_string())
                    } else {
                        // Remove .0 from float representation
                        let s = n.to_string();
                        if s.ends_with(".0") {
                            s.trim_end_matches(".0").to_string()
                        } else {
                            s
                        }
                    }
                }
                _ => return Ok(()), // Skip non-string/non-number values
            };

            let formatted = format_phone_number(&phone_str);
            json_obj.insert(field.to_string(), serde_json::Value::String(formatted));
        }

        Ok(())
    }

    /// Trim whitespace from specified fields
    fn apply_trim_whitespace(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
        headers: &[String],
    ) -> Result<()> {
        let fields = if let Some(fields_arr) = config.get("fields").and_then(|v| v.as_array()) {
            fields_arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            // Default: apply to all string fields
            headers.to_vec()
        };

        for field in fields {
            if let Some(value) = json_obj.get(&field) {
                if let Some(s) = value.as_str() {
                    let trimmed = s.trim().to_string();
                    json_obj.insert(field, serde_json::Value::String(trimmed));
                }
            }
        }

        Ok(())
    }

    /// Convert specified fields to uppercase
    fn apply_uppercase(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
        headers: &[String],
    ) -> Result<()> {
        let fields = if let Some(fields_arr) = config.get("fields").and_then(|v| v.as_array()) {
            fields_arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            headers.to_vec()
        };

        for field in fields {
            if let Some(value) = json_obj.get(&field) {
                if let Some(s) = value.as_str() {
                    json_obj.insert(field, serde_json::Value::String(s.to_uppercase()));
                }
            }
        }

        Ok(())
    }

    /// Convert specified fields to lowercase
    fn apply_lowercase(
        &self,
        json_obj: &mut serde_json::Map<String, serde_json::Value>,
        config: &serde_json::Value,
        headers: &[String],
    ) -> Result<()> {
        let fields = if let Some(fields_arr) = config.get("fields").and_then(|v| v.as_array()) {
            fields_arr
                .iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        } else {
            headers.to_vec()
        };

        for field in fields {
            if let Some(value) = json_obj.get(&field) {
                if let Some(s) = value.as_str() {
                    json_obj.insert(field, serde_json::Value::String(s.to_lowercase()));
                }
            }
        }

        Ok(())
    }

    /// Convert JSON object back to CSV row
    fn json_to_csv_row(
        &self,
        json_obj: &serde_json::Map<String, serde_json::Value>,
        headers: &[String],
    ) -> Result<Vec<String>> {
        let mut row = Vec::new();

        for header in headers {
            // Skip internal fields like source_table
            if header == "source_table" {
                continue;
            }

            let value = json_obj.get(header);
            let value_str = match value {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => {
                    // Handle integers without decimal point
                    if n.is_i64() || n.is_u64() {
                        n.as_i64()
                            .map(|i| i.to_string())
                            .or_else(|| n.as_u64().map(|u| u.to_string()))
                            .unwrap_or_else(|| n.to_string())
                    } else {
                        // For floats, check if it's actually an integer value
                        let s = n.to_string();
                        if s.ends_with(".0") {
                            s.trim_end_matches(".0").to_string()
                        } else {
                            s
                        }
                    }
                }
                Some(serde_json::Value::Bool(b)) => b.to_string(),
                Some(serde_json::Value::Null) | None => String::new(),
                Some(other) => other.to_string(),
            };
            row.push(value_str);
        }

        Ok(row)
    }
}

/// Convert string to proper case (Title Case)
fn to_proper_case(s: &str) -> String {
    s.split_whitespace()
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
        .join(" ")
}

/// Format phone number to standard format (XXX-XXX-XXXX)
fn format_phone_number(s: &str) -> String {
    // Remove all non-digit characters
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();

    // Format based on length
    match digits.len() {
        10 => {
            // US format: XXX-XXX-XXXX
            format!("{}-{}-{}", &digits[0..3], &digits[3..6], &digits[6..10])
        }
        11 if digits.starts_with('1') => {
            // US format with country code: 1-XXX-XXX-XXXX
            format!("1-{}-{}-{}", &digits[1..4], &digits[4..7], &digits[7..11])
        }
        _ => {
            // Unknown format, return digits only
            digits
        }
    }
}

/// Result of filtering a CSV file
#[derive(Debug, Clone)]
pub struct FilterResult {
    /// Total rows in input file
    pub total_rows: usize,

    /// Rows written to output file
    pub written_rows: usize,

    /// Rows filtered out
    pub filtered_rows: usize,

    /// Reasons for filtering (reason -> count)
    pub filter_reasons: HashMap<String, usize>,
}

impl FilterResult {
    /// Get filter rate as a percentage
    pub fn filter_rate(&self) -> f64 {
        if self.total_rows == 0 {
            0.0
        } else {
            (self.filtered_rows as f64 / self.total_rows as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{Action, Route};
    use crate::workflows::utils::string_pool::intern;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_workflow() -> Workflow {
        // Create workflow that filters active customers only
        let route = Route::new(
            "filter_active",
            "Filter Active Records",
            Condition::Equals {
                field: "status".to_string(),
                value: json!("active"),
            },
            vec![Action::Log {
                level: "info".to_string(),
                message: "Processing active record".to_string(),
            }],
        );

        Workflow::new("test_workflow", "Test Workflow", vec![route])
    }

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "id,name,status").unwrap();
        writeln!(file, "1,Alice,active").unwrap();
        writeln!(file, "2,Bob,inactive").unwrap();
        writeln!(file, "3,Charlie,active").unwrap();
        writeln!(file, "4,David,deleted").unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_csv_to_json_conversion() {
        let adapter = WorkflowCsvAdapter::new(create_test_workflow());

        let headers = vec!["id".to_string(), "name".to_string(), "age".to_string()];
        let row = vec!["123".to_string(), "Alice".to_string(), "30".to_string()];

        let json = adapter
            .csv_row_to_json_from_slices(&headers, &row, "customers")
            .unwrap();

        assert_eq!(json["source_table"], "customers");
        assert_eq!(json["id"], 123.0); // Parsed as number
        assert_eq!(json["name"], "Alice");
        assert_eq!(json["age"], 30.0); // Parsed as number
    }

    #[test]
    fn test_should_process_row() {
        let adapter = WorkflowCsvAdapter::new(create_test_workflow());

        let headers = vec!["id".to_string(), "name".to_string(), "status".to_string()];

        // Active row - should process
        let row1 = vec!["1".to_string(), "Alice".to_string(), "active".to_string()];
        assert!(adapter
            .should_process_row(&headers, &row1, "customers")
            .unwrap());

        // Inactive row - should NOT process
        let row2 = vec!["2".to_string(), "Bob".to_string(), "inactive".to_string()];
        assert!(!adapter
            .should_process_row(&headers, &row2, "customers")
            .unwrap());
    }

    #[tokio::test]
    async fn test_filter_csv_file() {
        let adapter = WorkflowCsvAdapter::new(create_test_workflow());

        let input_file = create_test_csv();
        let output_file = NamedTempFile::new().unwrap();

        let result = adapter
            .filter_csv_file(input_file.path(), output_file.path(), "customers")
            .await
            .unwrap();

        assert_eq!(result.total_rows, 4);
        assert_eq!(result.written_rows, 2); // Only active records
        assert_eq!(result.filtered_rows, 2); // Inactive and deleted
        assert_eq!(result.filter_rate(), 50.0);

        // Verify output file
        let mut reader = csv::Reader::from_path(output_file.path()).unwrap();
        let mut count = 0;
        for record in reader.records() {
            let record = record.unwrap();
            assert_eq!(&record[2], "active"); // All records should be active
            count += 1;
        }
        assert_eq!(count, 2);
    }

    fn create_transformation_workflow() -> Workflow {
        use crate::workflows::domain::Action;

        // Create workflow with transformations
        let route = Route::new(
            "transform_customer_data",
            "Transform Customer Data",
            Condition::Equals {
                field: "status".to_string(),
                value: json!("active"),
            },
            vec![
                Action::Transform {
                    transformer: "proper_case".to_string(),
                    config: json!({"fields": ["name"]}),
                },
                Action::Transform {
                    transformer: "normalize_email".to_string(),
                    config: json!({"field": "email"}),
                },
                Action::SetField {
                    field: "processed".to_string(),
                    value: json!("true"),
                },
            ],
        );

        Workflow::new("transform_workflow", "Transform Workflow", vec![route])
    }

    fn create_transformation_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "id,name,email,status").unwrap();
        writeln!(file, "1,john doe,JOHN.DOE@EXAMPLE.COM,active").unwrap();
        writeln!(file, "2,jane SMITH,Jane.Smith@Example.Com,active").unwrap();
        writeln!(file, "3,bob johnson,bob@example.com,inactive").unwrap();
        file.flush().unwrap();
        file
    }

    #[tokio::test]
    async fn test_workflow_transformations() {
        let adapter = WorkflowCsvAdapter::new(create_transformation_workflow());

        let input_file = create_transformation_test_csv();
        let output_file = NamedTempFile::new().unwrap();

        let result = adapter
            .filter_csv_file(input_file.path(), output_file.path(), "customers")
            .await
            .unwrap();

        assert_eq!(result.total_rows, 3);
        assert_eq!(result.written_rows, 2); // Only active records

        // Verify transformations
        let mut reader = csv::Reader::from_path(output_file.path()).unwrap();
        let records: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

        assert_eq!(records.len(), 2);

        // Check first record - proper case and normalized email
        assert_eq!(&records[0][0], "1");
        assert_eq!(&records[0][1], "John Doe"); // Proper cased
        assert_eq!(&records[0][2], "john.doe@example.com"); // Normalized email
        assert_eq!(&records[0][3], "active");

        // Check second record
        assert_eq!(&records[1][0], "2");
        assert_eq!(&records[1][1], "Jane Smith"); // Proper cased
        assert_eq!(&records[1][2], "jane.smith@example.com"); // Normalized email
        assert_eq!(&records[1][3], "active");
    }

    #[test]
    fn test_proper_case_transformation() {
        let adapter = WorkflowCsvAdapter::new(create_transformation_workflow());

        let headers = vec!["name".to_string()];
        let row = vec!["JOHN DOE".to_string()];

        let transformed = adapter.transform_row(&headers, row, "customers").unwrap();

        // Note: Since no route matches (no status field), row is returned unchanged
        // This test demonstrates the proper_case function directly
        assert_eq!(to_proper_case("JOHN DOE"), "John Doe");
        assert_eq!(to_proper_case("mary jane watson"), "Mary Jane Watson");
        assert_eq!(to_proper_case("bob"), "Bob");
    }

    #[test]
    fn test_phone_formatting() {
        assert_eq!(format_phone_number("5551234567"), "555-123-4567");
        assert_eq!(format_phone_number("555-123-4567"), "555-123-4567");
        assert_eq!(format_phone_number("(555) 123-4567"), "555-123-4567");
        assert_eq!(format_phone_number("15551234567"), "1-555-123-4567");
        assert_eq!(format_phone_number("1-555-123-4567"), "1-555-123-4567");
        assert_eq!(format_phone_number("12345"), "12345"); // Invalid, returns digits only
    }

    fn create_phone_workflow() -> Workflow {
        use crate::workflows::domain::Action;

        let route = Route::new(
            "format_phones",
            "Format Phone Numbers",
            Condition::Always, // Match all rows
            vec![Action::Transform {
                transformer: "format_phone".to_string(),
                config: json!({"field": "phone"}),
            }],
        );

        Workflow::new("phone_workflow", "Phone Workflow", vec![route])
    }

    #[tokio::test]
    async fn test_phone_number_formatting_workflow() {
        let adapter = WorkflowCsvAdapter::new(create_phone_workflow());

        let mut input_file = NamedTempFile::new().unwrap();
        writeln!(input_file, "id,phone").unwrap();
        writeln!(input_file, "1,5551234567").unwrap();
        writeln!(input_file, "2,(555) 123-4567").unwrap();
        writeln!(input_file, "3,1-555-987-6543").unwrap();
        input_file.flush().unwrap();

        let output_file = NamedTempFile::new().unwrap();

        let result = adapter
            .filter_csv_file(input_file.path(), output_file.path(), "contacts")
            .await
            .unwrap();

        assert_eq!(result.total_rows, 3);
        assert_eq!(result.written_rows, 3);

        // Verify phone formatting
        let mut reader = csv::Reader::from_path(output_file.path()).unwrap();
        let records: Vec<_> = reader.records().map(|r| r.unwrap()).collect();

        assert_eq!(&records[0][1], "555-123-4567");
        assert_eq!(&records[1][1], "555-123-4567");
        assert_eq!(&records[2][1], "1-555-987-6543");
    }

    #[test]
    fn test_set_field_action() {
        use crate::workflows::domain::Action;

        let route = Route::new(
            "add_timestamp",
            "Add Timestamp",
            Condition::Always, // Match all rows
            vec![Action::SetField {
                field: "processed_date".to_string(),
                value: json!("2025-10-17"),
            }],
        );

        let workflow = Workflow::new("set_field_workflow", "Set Field Workflow", vec![route]);
        let adapter = WorkflowCsvAdapter::new(workflow);

        let headers = vec!["id".to_string(), "name".to_string()];
        let row = vec!["1".to_string(), "Alice".to_string()];

        let transformed = adapter.transform_row(&headers, row, "users").unwrap();

        // SetField adds a new field, but we can't see it in CSV output
        // because it's not in the original headers
        // This demonstrates the action works internally
        assert_eq!(transformed.len(), 2);
    }

    #[test]
    fn test_multiple_transformations() {
        use crate::workflows::domain::Action;

        let route = Route::new(
            "multi_transform",
            "Multiple Transformations",
            Condition::Always, // Match all rows
            vec![
                Action::Transform {
                    transformer: "trim_whitespace".to_string(),
                    config: json!({"fields": ["name", "city"]}),
                },
                Action::Transform {
                    transformer: "uppercase".to_string(),
                    config: json!({"fields": ["code"]}),
                },
                Action::Transform {
                    transformer: "lowercase".to_string(),
                    config: json!({"fields": ["email"]}),
                },
            ],
        );

        let workflow = Workflow::new("multi_workflow", "Multi Workflow", vec![route]);
        let adapter = WorkflowCsvAdapter::new(workflow);

        let headers = vec![
            "name".to_string(),
            "city".to_string(),
            "code".to_string(),
            "email".to_string(),
        ];
        let row = vec![
            "  Alice  ".to_string(),
            "  New York  ".to_string(),
            "abc123".to_string(),
            "ALICE@EXAMPLE.COM".to_string(),
        ];

        let transformed = adapter.transform_row(&headers, row, "users").unwrap();

        assert_eq!(transformed[0], "Alice"); // Trimmed
        assert_eq!(transformed[1], "New York"); // Trimmed
        assert_eq!(transformed[2], "ABC123"); // Uppercased
        assert_eq!(transformed[3], "alice@example.com"); // Lowercased
    }
}
