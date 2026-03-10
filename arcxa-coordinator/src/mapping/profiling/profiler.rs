//! Source Profiler Implementation
//!
//! Wraps IncrementalProfiler with RDF serialization and async I/O.

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Instant;

use super::rdf::DcatSerializer;
use super::types::{
    ColumnProfile, DataType, DatasetUri, ProfileConfig, ProfileResult, ValueFrequency,
};
use crate::mapping::loader::orchestration::async_csv_reader::{
    AsyncCsvReader, AsyncCsvReaderConfig,
};

/// Source profiler that analyzes CSV/Parquet files and generates RDF metadata
pub struct SourceProfiler {
    /// Configuration for profiling
    config: ProfileConfig,

    /// RDF serializer (DCAT/VoID)
    rdf_serializer: DcatSerializer,
}

impl SourceProfiler {
    /// Create new source profiler
    pub fn new(config: ProfileConfig) -> Self {
        Self {
            config,
            rdf_serializer: DcatSerializer::new(),
        }
    }

    /// Profile a CSV file and return structured result
    pub async fn profile_csv(&self, file_path: &Path) -> Result<ProfileResult> {
        let start_time = Instant::now();

        tracing::info!("Profiling CSV file: {:?}", file_path);

        // Get file metadata
        let file_size = tokio::fs::metadata(file_path)
            .await
            .context("Failed to read file metadata")?
            .len();

        // Create CSV reader
        let csv_config = AsyncCsvReaderConfig {
            file_path: file_path.to_path_buf(),
            delimiter: b',',
            has_header: true,
            buffer_size: 8192,
            ..Default::default()
        };

        let mut reader = AsyncCsvReader::new(csv_config)
            .await
            .context("Failed to open CSV file")?;

        // Read header to get column names
        let headers = reader.headers().clone();
        let column_count = headers.len();

        // Initialize column profilers
        let mut column_profilers: Vec<ColumnProfilerState> = headers
            .iter()
            .enumerate()
            .map(|(index, name)| ColumnProfilerState::new(index, name.to_string()))
            .collect();

        let mut rows_profiled = 0u64;
        let sample_limit = self.config.sample_size.unwrap_or(usize::MAX);

        // Stream through CSV and build profiles
        while let Some(record) = reader.next_row().await? {
            if rows_profiled >= sample_limit as u64 {
                tracing::debug!("Reached sample limit: {}", sample_limit);
                break;
            }

            // Update each column profiler
            for (col_idx, value) in record.iter().enumerate() {
                if let Some(profiler) = column_profilers.get_mut(col_idx) {
                    profiler.observe_value(value);
                }
            }

            rows_profiled += 1;

            // Log progress every 10K rows
            if rows_profiled % 10_000 == 0 {
                tracing::debug!("Profiled {} rows", rows_profiled);
            }
        }

        // Finalize column profiles
        let columns: Vec<ColumnProfile> = column_profilers
            .into_iter()
            .map(|p| p.finalize(rows_profiled))
            .collect();

        // Identify candidate keys (high cardinality, low nulls)
        let candidate_keys = columns
            .iter()
            .filter(|col| {
                col.cardinality >= self.config.candidate_key_threshold && col.null_percentage < 0.01
                // Less than 1% nulls
            })
            .map(|col| col.name.clone())
            .collect();

        let duration = start_time.elapsed();

        let profile = ProfileResult {
            dataset_id: file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            source_location: file_path.display().to_string(),
            format: "csv".to_string(),
            file_size_bytes: file_size,
            total_rows: None, // Unknown without full scan
            rows_profiled,
            column_count,
            columns,
            candidate_keys,
            profiled_at: chrono::Utc::now(),
            duration_seconds: duration.as_secs_f64(),
        };

        tracing::info!(
            "Profiling complete: {} rows, {} columns in {:.1}s",
            rows_profiled,
            column_count,
            duration.as_secs_f64()
        );

        Ok(profile)
    }

    /// Serialize profile to RDF triples (DCAT/VoID vocabularies)
    pub fn profile_to_rdf(
        &self,
        profile: &ProfileResult,
        dataset_uri: &DatasetUri,
    ) -> Result<String> {
        self.rdf_serializer.serialize(profile, dataset_uri)
    }

    /// Generate dataset URI from file path
    pub fn generate_dataset_uri(&self, file_path: &Path) -> DatasetUri {
        DatasetUri::from_path(&file_path.to_path_buf())
    }
}

/// Internal state for profiling a single column
struct ColumnProfilerState {
    index: usize,
    name: String,

    // Counts
    null_count: u64,
    total_count: u64,

    // Type inference
    int_count: u64,
    float_count: u64,
    date_count: u64,
    bool_count: u64,

    // String metrics
    min_length: Option<usize>,
    max_length: Option<usize>,
    total_length: usize,

    // Numeric metrics (parsed as f64)
    numeric_values: Vec<f64>, // For mean/median/stdev (sample only)
    min_numeric: Option<f64>,
    max_numeric: Option<f64>,

    // Distinct count (simple HashSet for now, TODO: HyperLogLog)
    distinct_values: std::collections::HashSet<String>,

    // Frequency tracking (top 10)
    value_counts: std::collections::HashMap<String, u64>,
}

impl ColumnProfilerState {
    fn new(index: usize, name: String) -> Self {
        Self {
            index,
            name,
            null_count: 0,
            total_count: 0,
            int_count: 0,
            float_count: 0,
            date_count: 0,
            bool_count: 0,
            min_length: None,
            max_length: None,
            total_length: 0,
            numeric_values: Vec::new(),
            min_numeric: None,
            max_numeric: None,
            distinct_values: std::collections::HashSet::new(),
            value_counts: std::collections::HashMap::new(),
        }
    }

    fn observe_value(&mut self, value: &str) {
        self.total_count += 1;

        // Check for null/empty
        if value.is_empty()
            || value.eq_ignore_ascii_case("null")
            || value.eq_ignore_ascii_case("na")
        {
            self.null_count += 1;
            return;
        }

        // Track distinct values (TODO: Use HyperLogLog for large datasets)
        if self.distinct_values.len() < 10_000 {
            self.distinct_values.insert(value.to_string());
        }

        // Track value frequency (top 100)
        if self.value_counts.len() < 100 {
            *self.value_counts.entry(value.to_string()).or_insert(0) += 1;
        }

        // String metrics
        let len = value.len();
        self.min_length = Some(self.min_length.map_or(len, |min| min.min(len)));
        self.max_length = Some(self.max_length.map_or(len, |max| max.max(len)));
        self.total_length += len;

        // Type inference
        if value.parse::<i64>().is_ok() {
            self.int_count += 1;
            if let Ok(num) = value.parse::<f64>() {
                if self.numeric_values.len() < 10_000 {
                    self.numeric_values.push(num);
                }
                self.min_numeric = Some(self.min_numeric.map_or(num, |min| min.min(num)));
                self.max_numeric = Some(self.max_numeric.map_or(num, |max| max.max(num)));
            }
        } else if value.parse::<f64>().is_ok() {
            self.float_count += 1;
            if let Ok(num) = value.parse::<f64>() {
                if self.numeric_values.len() < 10_000 {
                    self.numeric_values.push(num);
                }
                self.min_numeric = Some(self.min_numeric.map_or(num, |min| min.min(num)));
                self.max_numeric = Some(self.max_numeric.map_or(num, |max| max.max(num)));
            }
        } else if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
            self.bool_count += 1;
        } else if chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
            self.date_count += 1;
        }
    }

    fn finalize(self, _total_rows: u64) -> ColumnProfile {
        let non_null_count = self.total_count - self.null_count;
        let null_percentage = if self.total_count > 0 {
            self.null_count as f64 / self.total_count as f64
        } else {
            0.0
        };

        // Infer data type based on counts
        let data_type = if self.int_count as f64 / non_null_count as f64 > 0.9 {
            DataType::Integer
        } else if self.float_count as f64 / non_null_count as f64 > 0.9 {
            DataType::Float
        } else if self.bool_count as f64 / non_null_count as f64 > 0.9 {
            DataType::Boolean
        } else if self.date_count as f64 / non_null_count as f64 > 0.9 {
            DataType::Date
        } else {
            DataType::String
        };

        // Calculate numeric statistics
        let (mean, median, std_dev) = if !self.numeric_values.is_empty() {
            let sum: f64 = self.numeric_values.iter().sum();
            let mean = sum / self.numeric_values.len() as f64;

            let mut sorted = self.numeric_values.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let median = sorted[sorted.len() / 2];

            let variance: f64 = self
                .numeric_values
                .iter()
                .map(|v| (v - mean).powi(2))
                .sum::<f64>()
                / self.numeric_values.len() as f64;
            let std_dev = variance.sqrt();

            (Some(mean), Some(median), Some(std_dev))
        } else {
            (None, None, None)
        };

        // Get top 10 values
        let mut top_values: Vec<ValueFrequency> = self
            .value_counts
            .into_iter()
            .map(|(value, count)| ValueFrequency {
                value,
                count,
                percentage: count as f64 / non_null_count as f64,
            })
            .collect();
        top_values.sort_by(|a, b| b.count.cmp(&a.count));
        top_values.truncate(10);

        let distinct_count = self.distinct_values.len() as u64;
        let cardinality = if non_null_count > 0 {
            distinct_count as f64 / non_null_count as f64
        } else {
            0.0
        };

        ColumnProfile {
            name: self.name,
            index: self.index,
            data_type,
            semantic_type: None, // TODO: Pattern-based detection
            null_count: self.null_count,
            null_percentage,
            distinct_count,
            cardinality,
            min_value: self.min_numeric.map(|v| v.to_string()),
            max_value: self.max_numeric.map(|v| v.to_string()),
            mean,
            median,
            std_dev,
            min_length: self.min_length,
            max_length: self.max_length,
            avg_length: if non_null_count > 0 {
                Some(self.total_length as f64 / non_null_count as f64)
            } else {
                None
            },
            pattern_example: None, // TODO: Regex pattern extraction
            pattern_regex: None,
            top_values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "id,name,age,email").unwrap();
        writeln!(file, "1,Alice,30,alice@example.com").unwrap();
        writeln!(file, "2,Bob,25,bob@example.com").unwrap();
        writeln!(file, "3,Charlie,35,charlie@example.com").unwrap();
        writeln!(file, "4,Diana,,diana@example.com").unwrap(); // Null age
        file.flush().unwrap();
        file
    }

    #[tokio::test]
    async fn test_csv_profiling() {
        let csv_file = create_test_csv();
        let profiler = SourceProfiler::new(ProfileConfig::default());

        let profile = profiler.profile_csv(csv_file.path()).await.unwrap();

        assert_eq!(profile.column_count, 4);
        assert_eq!(profile.rows_profiled, 4);

        // Check ID column
        let id_col = profile.columns.iter().find(|c| c.name == "id").unwrap();
        assert_eq!(id_col.data_type, DataType::Integer);
        assert_eq!(id_col.null_count, 0);
        assert_eq!(id_col.distinct_count, 4);

        // Check age column
        let age_col = profile.columns.iter().find(|c| c.name == "age").unwrap();
        assert_eq!(age_col.data_type, DataType::Integer);
        assert_eq!(age_col.null_count, 1); // Diana has null age
        assert!(age_col.mean.is_some());
    }
}
