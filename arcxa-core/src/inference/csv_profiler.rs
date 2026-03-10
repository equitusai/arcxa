//! CSV Field Profiler
//!
//! Analyzes CSV files to generate field profiles compatible with the field mapping engine.
//!
//! ## Features
//! - Data type inference (Integer, Float, String, Boolean, Date, Email, Phone)
//! - Cardinality analysis (distinct count estimation)
//! - Value distribution (min/max, quartiles)
//! - Null percentage calculation
//! - Sample value collection
//! - Neighbor field identification
//!
//! ## Integration
//! Produces `FieldMetadata` and `FieldProfile` structures that can be directly
//! used with the `FieldMapper` for intelligent field matching.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

use crate::inference::mapping::{
    DataType, DatasetSchema, FieldMetadata, FieldProfile, ValueDistribution,
};

/// CSV field profile statistics
#[derive(Debug, Clone)]
pub struct CsvFieldProfile {
    /// Column name
    pub name: String,

    /// Column position (0-indexed)
    pub position: usize,

    /// Inferred data type
    pub data_type: DataType,

    /// Total number of rows (excluding header)
    pub total_rows: u64,

    /// Number of null/empty values
    pub null_count: u64,

    /// Null percentage (0.0 - 1.0)
    pub null_percentage: f64,

    /// Number of distinct values (exact count)
    pub distinct_count: u64,

    /// Distinct values map (value -> count)
    pub value_counts: HashMap<String, u64>,

    /// Numeric values (if data type is Integer or Float)
    pub numeric_values: Vec<f64>,

    /// String values (for min/max lexicographic comparison)
    pub string_values: Vec<String>,

    /// Sample values (up to 10)
    pub samples: Vec<String>,
}

impl CsvFieldProfile {
    /// Create a new empty field profile
    pub fn new(name: String, position: usize) -> Self {
        Self {
            name,
            position,
            data_type: DataType::Unknown,
            total_rows: 0,
            null_count: 0,
            null_percentage: 0.0,
            distinct_count: 0,
            value_counts: HashMap::new(),
            numeric_values: Vec::new(),
            string_values: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// Add a value to the profile
    pub fn add_value(&mut self, value: &str) {
        self.total_rows += 1;

        // Check if null/empty
        if value.is_empty()
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("na")
        {
            self.null_count += 1;
            return;
        }

        // Track value count
        *self.value_counts.entry(value.to_string()).or_insert(0) += 1;

        // Add to samples (if < 10)
        if self.samples.len() < 10 && !self.samples.contains(&value.to_string()) {
            self.samples.push(value.to_string());
        }

        // Try to parse as numeric
        if let Ok(num) = value.parse::<f64>() {
            self.numeric_values.push(num);
        }

        // Always track as string for min/max
        self.string_values.push(value.to_string());
    }

    /// Finalize the profile (calculate derived statistics)
    pub fn finalize(&mut self) {
        // Calculate null percentage
        if self.total_rows > 0 {
            self.null_percentage = self.null_count as f64 / self.total_rows as f64;
        }

        // Calculate distinct count
        self.distinct_count = self.value_counts.len() as u64;

        // Infer data type
        self.data_type = self.infer_data_type();

        // Sort numeric values for quantile calculation
        if !self.numeric_values.is_empty() {
            self.numeric_values
                .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        }

        // Sort string values for lexicographic min/max
        self.string_values.sort();
    }

    /// Infer data type based on values
    fn infer_data_type(&self) -> DataType {
        let non_null_count = self.total_rows - self.null_count;

        if non_null_count == 0 {
            return DataType::Unknown;
        }

        let numeric_ratio = self.numeric_values.len() as f64 / non_null_count as f64;

        // If > 80% of non-null values are numeric
        if numeric_ratio > 0.8 {
            // Check if all numeric values are integers
            let all_integers = self.numeric_values.iter().all(|v| v.fract() == 0.0);
            if all_integers {
                return DataType::Integer;
            } else {
                return DataType::Float;
            }
        }

        // Check for boolean values
        let boolean_values = ["true", "false", "yes", "no", "0", "1", "y", "n"];
        let boolean_ratio = self
            .value_counts
            .keys()
            .filter(|v| boolean_values.contains(&v.to_lowercase().as_str()))
            .count() as f64
            / self.value_counts.len() as f64;

        if boolean_ratio > 0.8 && self.distinct_count <= 2 {
            return DataType::Boolean;
        }

        // Default to string
        // TODO: Add email/phone detection when Email and Phone DataTypes are added
        DataType::String
    }

    /// Convert to FieldProfile for field mapping
    pub fn to_field_profile(&self) -> FieldProfile {
        let distribution = if !self.numeric_values.is_empty() {
            ValueDistribution {
                min: self.numeric_values.first().map(|v| v.to_string()),
                max: self.numeric_values.last().map(|v| v.to_string()),
                median: self.calculate_percentile(0.5).map(|v| v.to_string()),
                p25: self.calculate_percentile(0.25).map(|v| v.to_string()),
                p75: self.calculate_percentile(0.75).map(|v| v.to_string()),
                ..Default::default()
            }
        } else if !self.string_values.is_empty() {
            ValueDistribution {
                min: self.string_values.first().cloned(),
                max: self.string_values.last().cloned(),
                ..Default::default()
            }
        } else {
            ValueDistribution::default()
        };

        FieldProfile {
            distinct_count: self.distinct_count,
            total_rows: self.total_rows,
            null_percentage: self.null_percentage,
            distribution,
            samples: self.samples.clone(),
        }
    }

    /// Calculate percentile for numeric values
    fn calculate_percentile(&self, percentile: f64) -> Option<f64> {
        if self.numeric_values.is_empty() {
            return None;
        }

        let index = (percentile * (self.numeric_values.len() - 1) as f64).round() as usize;
        Some(self.numeric_values[index])
    }
}

/// CSV profiler configuration
#[derive(Debug, Clone)]
pub struct CsvProfilerConfig {
    /// Maximum number of rows to sample for profiling
    /// None = profile entire file
    pub max_rows: Option<usize>,

    /// CSV delimiter
    pub delimiter: u8,

    /// Whether file has header row
    pub has_header: bool,
}

impl Default for CsvProfilerConfig {
    fn default() -> Self {
        Self {
            max_rows: Some(100_000), // Sample up to 100K rows by default
            delimiter: b',',
            has_header: true,
        }
    }
}

/// Profile a CSV file and generate FieldMetadata for field mapping
pub fn profile_csv_file(file_path: &Path, config: CsvProfilerConfig) -> Result<DatasetSchema> {
    use csv::ReaderBuilder;
    use std::fs::File;
    use std::io::BufReader;

    // Open file
    let file = File::open(file_path)
        .with_context(|| format!("Failed to open CSV file: {:?}", file_path))?;

    let buf_reader = BufReader::with_capacity(8 * 1024 * 1024, file); // 8MB buffer

    // Build CSV reader
    let mut csv_reader = ReaderBuilder::new()
        .delimiter(config.delimiter)
        .has_headers(config.has_header)
        .flexible(true) // Allow variable number of fields
        .from_reader(buf_reader);

    // Read header
    let headers = if config.has_header {
        csv_reader
            .headers()
            .context("Failed to read CSV header")?
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        // Generate column names: col0, col1, col2, ...
        let first_record = csv_reader
            .records()
            .next()
            .ok_or_else(|| anyhow::anyhow!("CSV file is empty"))??;
        (0..first_record.len())
            .map(|i| format!("col{}", i))
            .collect()
    };

    let num_columns = headers.len();

    // Initialize profiles for each column
    let mut profiles: Vec<CsvFieldProfile> = headers
        .iter()
        .enumerate()
        .map(|(pos, name)| CsvFieldProfile::new(name.clone(), pos))
        .collect();

    // Profile rows
    let mut row_count = 0;
    for result in csv_reader.records() {
        let record = result.context("Failed to read CSV record")?;

        // Add values to each column profile
        for (col_idx, value) in record.iter().enumerate() {
            if col_idx < num_columns {
                profiles[col_idx].add_value(value);
            }
        }

        row_count += 1;

        // Check max_rows limit
        if let Some(max) = config.max_rows {
            if row_count >= max {
                tracing::info!("Reached max_rows limit ({}) for profiling", max);
                break;
            }
        }
    }

    // Finalize all profiles
    for profile in &mut profiles {
        profile.finalize();
    }

    // Convert to FieldMetadata for field mapping
    let fields: Vec<FieldMetadata> = profiles
        .iter()
        .map(|prof| {
            let neighbors = calculate_neighbors(&headers, prof.position);

            FieldMetadata {
                qualified_name: format!(
                    "{}.{}",
                    file_path.file_name().unwrap_or_default().to_string_lossy(),
                    prof.name
                ),
                column_name: prof.name.clone(),
                source_id: file_path.to_string_lossy().to_string(),
                data_type: prof.data_type.clone(),
                profile: prof.to_field_profile(),
                semantic_type: None, // TODO: Add semantic type detection
                position: prof.position,
                neighbors,
            }
        })
        .collect();

    Ok(DatasetSchema {
        dataset_id: file_path.to_string_lossy().to_string(),
        dataset_name: file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        fields,
    })
}

/// Calculate neighboring field names
fn calculate_neighbors(headers: &[String], position: usize) -> Vec<String> {
    let mut neighbors = Vec::new();

    // Add previous column (if exists)
    if position > 0 {
        neighbors.push(headers[position - 1].clone());
    }

    // Add next column (if exists)
    if position + 1 < headers.len() {
        neighbors.push(headers[position + 1].clone());
    }

    neighbors
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_csv(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_profile_simple_csv() -> Result<()> {
        let csv_content = "name,age,email\nAlice,30,alice@example.com\nBob,25,bob@example.com\nCharlie,35,charlie@example.com\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        // Verify dataset
        assert_eq!(schema.fields.len(), 3);

        // Verify name column
        let name_field = &schema.fields[0];
        assert_eq!(name_field.column_name, "name");
        assert_eq!(name_field.data_type, DataType::String);
        assert_eq!(name_field.profile.total_rows, 3);
        assert_eq!(name_field.profile.distinct_count, 3);
        assert_eq!(name_field.position, 0);

        // Verify age column
        let age_field = &schema.fields[1];
        assert_eq!(age_field.column_name, "age");
        assert_eq!(age_field.data_type, DataType::Integer);
        assert_eq!(age_field.profile.total_rows, 3);
        assert_eq!(age_field.profile.distinct_count, 3);

        // Verify email column
        let email_field = &schema.fields[2];
        assert_eq!(email_field.column_name, "email");
        assert_eq!(email_field.data_type, DataType::String); // TODO: Change to Email when type is added
        assert_eq!(email_field.profile.total_rows, 3);

        Ok(())
    }

    #[test]
    fn test_data_type_inference() -> Result<()> {
        let csv_content = "int_col,float_col,string_col,bool_col\n1,1.5,hello,true\n2,2.5,world,false\n3,3.5,foo,true\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        assert_eq!(schema.fields[0].data_type, DataType::Integer);
        assert_eq!(schema.fields[1].data_type, DataType::Float);
        assert_eq!(schema.fields[2].data_type, DataType::String);
        assert_eq!(schema.fields[3].data_type, DataType::Boolean);

        Ok(())
    }

    #[test]
    fn test_null_percentage() -> Result<()> {
        let csv_content = "col1,col2\n1,a\n,b\n3,\n4,d\n,\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        // col1: 2 nulls out of 5 = 40%
        assert_eq!(schema.fields[0].profile.null_percentage, 0.4);

        // col2: 2 nulls out of 5 = 40%
        assert_eq!(schema.fields[1].profile.null_percentage, 0.4);

        Ok(())
    }

    #[test]
    fn test_cardinality_tracking() -> Result<()> {
        let csv_content = "unique_col,duplicate_col\n1,a\n2,b\n3,a\n4,b\n5,a\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        // unique_col: 5 distinct values
        assert_eq!(schema.fields[0].profile.distinct_count, 5);

        // duplicate_col: 2 distinct values (a, b)
        assert_eq!(schema.fields[1].profile.distinct_count, 2);

        Ok(())
    }

    #[test]
    fn test_value_distribution() -> Result<()> {
        let csv_content = "numbers\n10\n20\n30\n40\n50\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        let field = &schema.fields[0];
        assert_eq!(field.profile.distribution.min, Some("10".to_string()));
        assert_eq!(field.profile.distribution.max, Some("50".to_string()));
        assert_eq!(field.profile.distribution.median, Some("30".to_string()));

        Ok(())
    }

    #[test]
    fn test_neighbors() -> Result<()> {
        let csv_content = "col1,col2,col3\n1,2,3\n";
        let file = create_test_csv(csv_content);

        let config = CsvProfilerConfig::default();
        let schema = profile_csv_file(file.path(), config)?;

        // col1: neighbor is col2
        assert_eq!(schema.fields[0].neighbors, vec!["col2"]);

        // col2: neighbors are col1 and col3
        assert_eq!(schema.fields[1].neighbors, vec!["col1", "col3"]);

        // col3: neighbor is col2
        assert_eq!(schema.fields[2].neighbors, vec!["col2"]);

        Ok(())
    }

    #[test]
    fn test_max_rows_limit() -> Result<()> {
        // Create CSV with 100 rows
        let mut csv_content = "col1\n".to_string();
        for i in 1..=100 {
            csv_content.push_str(&format!("{}\n", i));
        }

        let file = create_test_csv(&csv_content);

        let config = CsvProfilerConfig {
            max_rows: Some(50), // Only profile first 50 rows
            ..Default::default()
        };

        let schema = profile_csv_file(file.path(), config)?;

        // Should have profiled only 50 rows
        assert_eq!(schema.fields[0].profile.total_rows, 50);
        assert_eq!(schema.fields[0].profile.distinct_count, 50);

        Ok(())
    }
}
