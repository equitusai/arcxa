//! CSV Source executor - Read CSV files with schema detection
//!
//! Leverages the existing CSV connector from the catalog module to read
//! CSV files and automatically detect schema.

use anyhow::{Context, Result};
use graphica_core::orchestration::workflow::CsvSourceConfig;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, BufReader};

/// CSV Source executor
pub struct CsvSourceExecutor {
    config: CsvSourceConfig,
}

impl CsvSourceExecutor {
    pub fn new(config: CsvSourceConfig) -> Self {
        Self { config }
    }

    /// Scan CSV file and detect schema
    pub async fn scan(&self) -> Result<CsvScanResult> {
        let file_path = Path::new(&self.config.file_path);

        if !file_path.exists() {
            anyhow::bail!("CSV file not found: {}", self.config.file_path);
        }

        let file = File::open(file_path)
            .await
            .context("Failed to open CSV file")?;

        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        // Determine delimiter
        let delimiter = self.config.delimiter.unwrap_or(',');

        // Read header
        let has_header = self.config.has_header.unwrap_or(true);
        let mut field_names = Vec::new();
        let mut sample_values: Vec<Vec<String>> = Vec::new();

        let mut line_count = 0;
        let skip_rows = self.config.skip_rows.unwrap_or(0);
        let max_rows = self.config.max_rows.unwrap_or(1000);

        while let Some(line) = lines.next_line().await? {
            line_count += 1;

            // Skip initial rows if requested
            if line_count <= skip_rows {
                continue;
            }

            let values: Vec<String> = line
                .split(delimiter)
                .map(|s| s.trim().to_string())
                .collect();

            if has_header && field_names.is_empty() {
                // First line is header
                field_names = values;
                continue;
            }

            if !has_header && field_names.is_empty() {
                // Generate field names
                field_names = (0..values.len())
                    .map(|i| format!("column_{}", i + 1))
                    .collect();
            }

            // Collect sample values
            if sample_values.len() < 100 {
                // Store sample for each field
                for (i, value) in values.iter().enumerate() {
                    if sample_values.len() <= i {
                        sample_values.push(Vec::new());
                    }
                    if sample_values[i].len() < 10 {
                        sample_values[i].push(value.clone());
                    }
                }
            }

            if line_count - skip_rows >= max_rows {
                break;
            }
        }

        // Infer data types from sample values
        let detected_fields = field_names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let samples = sample_values.get(i).cloned().unwrap_or_default();
                let data_type = infer_type(&samples);

                DetectedField {
                    name: name.clone(),
                    data_type,
                    sample_values: samples.clone(),
                    nullable: samples.iter().any(|s| s.is_empty()),
                }
            })
            .collect();

        Ok(CsvScanResult {
            file_path: self.config.file_path.clone(),
            delimiter,
            has_header,
            encoding: self
                .config
                .encoding
                .clone()
                .unwrap_or_else(|| "UTF-8".to_string()),
            rows_scanned: line_count - skip_rows - (if has_header { 1 } else { 0 }),
            detected_fields,
            last_scanned: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Import CSV data as JSON records
    pub async fn import(&self) -> Result<Vec<Value>> {
        let file_path = Path::new(&self.config.file_path);
        let file = File::open(file_path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let delimiter = self.config.delimiter.unwrap_or(',');
        let has_header = self.config.has_header.unwrap_or(true);
        let skip_rows = self.config.skip_rows.unwrap_or(0);
        let max_rows = self.config.max_rows;

        let mut field_names = Vec::new();
        let mut records = Vec::new();
        let mut line_count = 0;

        while let Some(line) = lines.next_line().await? {
            line_count += 1;

            if line_count <= skip_rows {
                continue;
            }

            let values: Vec<String> = line
                .split(delimiter)
                .map(|s| s.trim().to_string())
                .collect();

            if has_header && field_names.is_empty() {
                field_names = values;
                continue;
            }

            if !has_header && field_names.is_empty() {
                field_names = (0..values.len())
                    .map(|i| format!("column_{}", i + 1))
                    .collect();
            }

            // Create JSON record
            let mut record = serde_json::Map::new();
            for (i, field_name) in field_names.iter().enumerate() {
                let value = values.get(i).cloned().unwrap_or_default();
                record.insert(field_name.clone(), Value::String(value));
            }
            records.push(Value::Object(record));

            if let Some(max) = max_rows {
                if records.len() >= max {
                    break;
                }
            }
        }

        Ok(records)
    }

    /// Preview CSV data (first N rows)
    pub async fn preview(&self, limit: usize) -> Result<CsvPreviewResult> {
        let mut config = self.config.clone();
        config.max_rows = Some(limit);

        let executor = CsvSourceExecutor::new(config);
        let records = executor.import().await?;
        let total_rows = records.len();

        Ok(CsvPreviewResult {
            records,
            total_rows,
        })
    }
}

#[async_trait::async_trait]
impl crate::etl::EtlExecutor for CsvSourceExecutor {
    async fn execute(&self, _input: Value) -> Result<Value> {
        let records = self.import().await?;
        Ok(json!({
            "records": records,
            "count": records.len(),
            "file_path": self.config.file_path,
        }))
    }

    fn step_type(&self) -> &'static str {
        "csv_source"
    }
}

/// CSV scan result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CsvScanResult {
    pub file_path: String,
    pub delimiter: char,
    pub has_header: bool,
    pub encoding: String,
    pub rows_scanned: usize,
    pub detected_fields: Vec<DetectedField>,
    pub last_scanned: String,
}

/// Detected field metadata
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DetectedField {
    pub name: String,
    pub data_type: String,
    pub sample_values: Vec<String>,
    pub nullable: bool,
}

/// CSV preview result
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CsvPreviewResult {
    pub records: Vec<Value>,
    pub total_rows: usize,
}

/// Infer data type from sample values
fn infer_type(samples: &[String]) -> String {
    if samples.is_empty() {
        return "STRING".to_string();
    }

    let mut all_integers = true;
    let mut all_floats = true;
    let mut all_booleans = true;
    let mut all_dates = true;

    for sample in samples {
        if sample.is_empty() {
            continue;
        }

        // Try integer
        if all_integers && sample.parse::<i64>().is_err() {
            all_integers = false;
        }

        // Try float
        if all_floats && sample.parse::<f64>().is_err() {
            all_floats = false;
        }

        // Try boolean
        if all_booleans {
            let lower = sample.to_lowercase();
            if !matches!(lower.as_str(), "true" | "false" | "1" | "0" | "yes" | "no") {
                all_booleans = false;
            }
        }

        // Try date (basic ISO 8601 check)
        if all_dates {
            if let Err(_) = chrono::NaiveDate::parse_from_str(sample, "%Y-%m-%d") {
                if let Err(_) = chrono::DateTime::parse_from_rfc3339(sample) {
                    all_dates = false;
                }
            }
        }
    }

    if all_integers {
        "INTEGER".to_string()
    } else if all_floats {
        "FLOAT".to_string()
    } else if all_booleans {
        "BOOLEAN".to_string()
    } else if all_dates {
        "DATE".to_string()
    } else {
        "STRING".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_type_integer() {
        let samples = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        assert_eq!(infer_type(&samples), "INTEGER");
    }

    #[test]
    fn test_infer_type_float() {
        let samples = vec!["1.5".to_string(), "2.7".to_string(), "3.14".to_string()];
        assert_eq!(infer_type(&samples), "FLOAT");
    }

    #[test]
    fn test_infer_type_string() {
        let samples = vec!["hello".to_string(), "world".to_string()];
        assert_eq!(infer_type(&samples), "STRING");
    }

    #[test]
    fn test_infer_type_boolean() {
        let samples = vec!["true".to_string(), "false".to_string(), "1".to_string()];
        assert_eq!(infer_type(&samples), "BOOLEAN");
    }
}
