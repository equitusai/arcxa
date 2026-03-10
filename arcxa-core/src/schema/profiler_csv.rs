//! CSV File Profiler
//!
//! Implements DataProfiler trait for CSV files using existing profiling logic

use super::profiler::*;
use super::semantic_detector::SemanticDetector;
use super::*;
use crate::inference::mapping::DataType as MappingDataType;
use crate::inference::{profile_csv_file, CsvProfilerConfig, DatasetSchema};
use anyhow::{Context, Result};
use std::path::Path;

/// CSV-specific profiler implementation
pub struct CsvProfiler {
    /// Delimiter character (default: ',')
    pub delimiter: u8,
    /// Whether CSV has header row
    pub has_header: bool,
    /// Semantic type detector
    pub semantic_detector: SemanticDetector,
}

impl Default for CsvProfiler {
    fn default() -> Self {
        Self {
            delimiter: b',',
            has_header: true,
            semantic_detector: SemanticDetector::new(),
        }
    }
}

impl CsvProfiler {
    /// Create a new CSV profiler
    pub fn new() -> Self {
        Self::default()
    }

    /// Create CSV profiler with custom delimiter
    pub fn with_delimiter(delimiter: u8) -> Self {
        Self {
            delimiter,
            has_header: true,
            semantic_detector: SemanticDetector::new(),
        }
    }

    /// Convert DatasetSchema to UnifiedSchema
    fn convert_to_unified(&self, dataset_schema: DatasetSchema, file_path: &str) -> UnifiedSchema {
        let mut schema = UnifiedSchema::new(
            dataset_schema.dataset_name.clone(),
            SourceType::CsvFile,
            file_path.to_string(),
        );

        // Get row count before moving fields
        let row_count = dataset_schema
            .fields
            .first()
            .map(|f| f.profile.total_rows)
            .unwrap_or(0);

        // Convert fields
        for field_meta in dataset_schema.fields {
            let mut unified_field = UnifiedField::new(
                field_meta.column_name.clone(),
                convert_mapping_data_type(&field_meta.data_type),
            );

            unified_field.position = field_meta.position;
            unified_field.nullable = field_meta.profile.null_percentage > 0.0;

            // Convert field profile from mapping::FieldProfile to schema::FieldProfile
            unified_field.profile = Some(convert_field_profile(&field_meta.profile));

            // Use semantic detector to automatically detect semantic types
            let sample_values: Vec<Option<String>> = field_meta
                .profile
                .samples
                .iter()
                .map(|s| Some(s.clone()))
                .collect();

            if let Some(detection_result) = self
                .semantic_detector
                .detect(&field_meta.column_name, &sample_values)
            {
                // Apply detected semantic type
                unified_field.semantic.semantic_type = Some(detection_result.semantic_type);

                // Apply suggested sensitivity level
                if let Some(sensitivity) = detection_result.suggested_sensitivity {
                    unified_field.semantic.sensitivity = Some(sensitivity);
                }

                // Update last classified timestamp
                unified_field.semantic.last_classified = Some(chrono::Utc::now());
            } else if let Some(ref semantic) = field_meta.semantic_type {
                // Fall back to parsing existing semantic type if no automatic detection
                unified_field.semantic.semantic_type = Some(parse_semantic_type(semantic));
            }

            schema.add_field(unified_field);
        }

        // Add metadata
        schema.row_count = Some(row_count);
        schema.last_profiled = Some(chrono::Utc::now());

        schema
    }
}

/// Convert mapping::DataType to UniversalDataType
fn convert_mapping_data_type(data_type: &MappingDataType) -> UniversalDataType {
    match data_type {
        MappingDataType::Integer => UniversalDataType::Integer { bits: Some(64) },
        MappingDataType::Float => UniversalDataType::Float { bits: Some(64) },
        MappingDataType::String => UniversalDataType::String { max_length: None },
        MappingDataType::Boolean => UniversalDataType::Boolean,
        MappingDataType::Date => UniversalDataType::Date,
        MappingDataType::DateTime => UniversalDataType::DateTime {
            with_timezone: false,
        },
        MappingDataType::Time => UniversalDataType::Time {
            with_timezone: false,
        },
        MappingDataType::Decimal { precision, scale } => UniversalDataType::Decimal {
            precision: *precision,
            scale: *scale,
        },
        MappingDataType::Binary => UniversalDataType::Binary { max_length: None },
        MappingDataType::Json => UniversalDataType::Json,
        MappingDataType::Unknown => UniversalDataType::Unknown,
    }
}

/// Convert mapping::FieldProfile to schema::FieldProfile
fn convert_field_profile(
    mapping_profile: &crate::inference::mapping::FieldProfile,
) -> FieldProfile {
    use crate::schema::profile::*;

    FieldProfile {
        distinct_count: mapping_profile.distinct_count,
        total_rows: mapping_profile.total_rows,
        null_count: (mapping_profile.total_rows as f64 * mapping_profile.null_percentage) as u64,
        null_percentage: mapping_profile.null_percentage,
        distribution: convert_value_distribution(&mapping_profile.distribution),
        samples: mapping_profile.samples.clone(),
        top_values: None, // Not provided by mapping profiler
        patterns: None,   // Not provided by mapping profiler
        quality: DataQualityMetrics {
            completeness: 1.0 - mapping_profile.null_percentage,
            uniqueness: if mapping_profile.total_rows > 0 {
                mapping_profile.distinct_count as f64 / mapping_profile.total_rows as f64
            } else {
                0.0
            },
            validity: 1.0,    // Would need actual validation
            consistency: 1.0, // Would need consistency checks
            overall_score: 1.0 - mapping_profile.null_percentage,
            issues: Vec::new(),
        },
    }
}

/// Convert mapping::ValueDistribution to schema::ValueDistribution
fn convert_value_distribution(
    mapping_dist: &crate::inference::mapping::ValueDistribution,
) -> crate::schema::profile::ValueDistribution {
    crate::schema::profile::ValueDistribution {
        min: mapping_dist.min.clone(),
        max: mapping_dist.max.clone(),
        mean: None, // Not provided by mapping profiler
        median: mapping_dist.median.clone(),
        mode: None,
        stddev: None,
        variance: None,
        p01: None,
        p05: None,
        p25: mapping_dist.p25.clone(),
        p50: mapping_dist.median.clone(),
        p75: mapping_dist.p75.clone(),
        p95: mapping_dist.p95.clone(),
        p99: mapping_dist.p99.clone(),
        sum: None,
        skewness: None,
        kurtosis: None,
    }
}

/// Parse semantic type string to SemanticType
fn parse_semantic_type(semantic: &str) -> SemanticType {
    use crate::schema::field::SemanticType;

    match semantic.to_lowercase().as_str() {
        "email" => SemanticType::Email,
        "phonenumber" | "phone" => SemanticType::PhoneNumber,
        "personname" | "name" => SemanticType::FullName,
        "companyname" | "organizationname" => SemanticType::CompanyName,
        "currency" | "currencyamount" => SemanticType::Currency,
        "percentage" => SemanticType::Percentage,
        _ => SemanticType::Custom(semantic.to_string()),
    }
}

impl DataProfiler for CsvProfiler {
    fn profile_source(
        &self,
        source_ref: &str,
        config: ProfileConfig,
    ) -> Result<Vec<UnifiedSchema>> {
        // For CSV, source_ref is the file path
        let schema = self.profile_table(source_ref, "", config)?;
        Ok(vec![schema])
    }

    fn profile_table(
        &self,
        source_ref: &str,
        _table_name: &str, // Ignored for CSV (single file)
        config: ProfileConfig,
    ) -> Result<UnifiedSchema> {
        let path = Path::new(source_ref);

        // Create CsvProfilerConfig from ProfileConfig
        let csv_config = CsvProfilerConfig {
            max_rows: config.sample_size,
            delimiter: self.delimiter,
            has_header: self.has_header,
        };

        // Use existing CSV profiling logic
        let dataset_schema = profile_csv_file(path, csv_config)
            .with_context(|| format!("Failed to profile CSV file: {}", source_ref))?;

        // Convert to unified schema
        Ok(self.convert_to_unified(dataset_schema, source_ref))
    }

    fn get_sample_data(
        &self,
        source_ref: &str,
        _table_name: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SampleRow>> {
        use csv::ReaderBuilder;

        let mut reader = ReaderBuilder::new()
            .delimiter(self.delimiter)
            .has_headers(self.has_header)
            .from_path(source_ref)
            .with_context(|| format!("Failed to open CSV file: {}", source_ref))?;

        let headers = if self.has_header {
            reader
                .headers()
                .context("Failed to read CSV headers")?
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        } else {
            // Generate default headers
            (0..reader.headers().unwrap().len())
                .map(|i| format!("column_{}", i))
                .collect()
        };

        let mut samples = Vec::new();

        for (i, result) in reader.records().enumerate() {
            if i >= limit {
                break;
            }

            let record = result.context("Failed to read CSV record")?;
            let mut row = std::collections::HashMap::new();

            for (j, value) in record.iter().enumerate() {
                if let Some(header) = headers.get(j) {
                    row.insert(header.clone(), serde_json::Value::String(value.to_string()));
                }
            }

            samples.push(row);
        }

        Ok(samples)
    }

    fn validate_quality(
        &self,
        source_ref: &str,
        _table_name: Option<&str>,
    ) -> Result<QualityReport> {
        use crate::schema::profile::{
            IssueSeverity, QualityIssue as ProfileQualityIssue, QualityIssueType,
        };

        // Basic quality validation for CSV
        let schema = self.profile_table(source_ref, "", ProfileConfig::default())?;

        let mut report = QualityReport::default();
        let mut total_null_count = 0;
        let mut total_cells = 0;

        for field in &schema.fields {
            if let Some(ref profile) = field.profile {
                total_null_count += profile.null_count;
                total_cells += profile.total_rows as usize;

                // Check for high null percentage
                if profile.null_percentage > 0.5 {
                    report.issues.push(profiler::QualityIssue {
                        field_name: Some(field.name.clone()),
                        severity: profiler::IssueSeverity::Warning,
                        description: format!(
                            "High null percentage: {:.1}%",
                            profile.null_percentage * 100.0
                        ),
                        affected_rows: profile.null_count as usize,
                    });
                }
            }
        }

        // Calculate completeness
        if total_cells > 0 {
            report.completeness = 1.0 - (total_null_count as f64 / total_cells as f64);
        }

        // Overall quality score (simplified)
        report.quality_score = report.completeness;
        report.validity = 1.0; // Would need type checking for accurate validity
        report.uniqueness = 1.0; // Would need uniqueness analysis

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name,age,email").unwrap();
        writeln!(file, "Alice,30,alice@example.com").unwrap();
        writeln!(file, "Bob,25,bob@example.com").unwrap();
        writeln!(file, "Charlie,35,").unwrap(); // null email
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_csv_profiler_creation() {
        let profiler = CsvProfiler::new();
        assert_eq!(profiler.delimiter, b',');
        assert!(profiler.has_header);

        let profiler = CsvProfiler::with_delimiter(b'\t');
        assert_eq!(profiler.delimiter, b'\t');
    }

    #[test]
    fn test_csv_profiler_profile_table() {
        let file = create_test_csv();
        let profiler = CsvProfiler::new();
        let config = ProfileConfig::default();

        let schema = profiler
            .profile_table(file.path().to_str().unwrap(), "", config)
            .unwrap();

        assert_eq!(schema.source_type, SourceType::CsvFile);
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.row_count, Some(3));

        // Check field names
        assert_eq!(schema.fields[0].name, "name");
        assert_eq!(schema.fields[1].name, "age");
        assert_eq!(schema.fields[2].name, "email");
    }

    #[test]
    fn test_csv_profiler_sample_data() {
        let file = create_test_csv();
        let profiler = CsvProfiler::new();

        let samples = profiler
            .get_sample_data(file.path().to_str().unwrap(), None, 2)
            .unwrap();

        assert_eq!(samples.len(), 2);
        assert!(samples[0].contains_key("name"));
        assert!(samples[0].contains_key("age"));
        assert!(samples[0].contains_key("email"));
    }

    #[test]
    fn test_csv_profiler_quality_report() {
        let file = create_test_csv();
        let profiler = CsvProfiler::new();

        let report = profiler
            .validate_quality(file.path().to_str().unwrap(), None)
            .unwrap();

        assert!(report.completeness > 0.0);
        assert!(report.completeness < 1.0); // Due to null email
    }
}
