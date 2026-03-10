//! # CSV Schema Extractor
//!
//! Extracts schema from CSV files using the existing csv_utils infrastructure.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;

use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;

use super::super::types::*;
use super::traits::SchemaExtractor;
use crate::common::csv_utils::analysis::{FieldTypeInference, SchemaInferenceConfig};

/// CSV file schema extractor
///
/// Extracts schema from CSV files by:
/// - Reading CSV headers for column names
/// - Sampling rows for type inference
/// - Detecting patterns (email, phone, SSN, etc.)
/// - Computing column statistics
pub struct CsvExtractor;

impl CsvExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract schema from CSV file path
    pub async fn extract_from_path(
        &self,
        csv_path: &Path,
        table_name: &str,
        sample_size: usize,
    ) -> Result<DiscoveredTable> {
        use csv::Reader;

        let mut reader = Reader::from_path(csv_path)
            .with_context(|| format!("Failed to open CSV file: {}", csv_path.display()))?;

        // Get headers
        let headers = reader.headers()?.clone();

        // Sample rows for type inference
        let mut rows_by_column: Vec<Vec<String>> = vec![Vec::new(); headers.len()];
        let mut row_count = 0;

        for result in reader.records().take(sample_size) {
            let record = result?;
            row_count += 1;

            for (col_idx, value) in record.iter().enumerate() {
                if col_idx < rows_by_column.len() {
                    rows_by_column[col_idx].push(value.to_string());
                }
            }
        }

        // Infer types for each column
        let config = SchemaInferenceConfig::default();
        let type_inference = FieldTypeInference::new(config);
        let mut columns = Vec::new();

        for (col_idx, header) in headers.iter().enumerate() {
            let values = &rows_by_column[col_idx];
            let (field_type, type_confidence) = type_inference.infer_field_type(values);

            // Calculate null count
            let null_count = values.iter().filter(|v| v.trim().is_empty()).count();
            let nullable = null_count > 0;

            // Calculate statistics
            let distinct_count = values
                .iter()
                .filter(|v| !v.trim().is_empty())
                .collect::<std::collections::HashSet<_>>()
                .len() as i64;

            let null_fraction = if row_count > 0 {
                null_count as f64 / row_count as f64
            } else {
                0.0
            };

            // Get sample values (first 10 non-empty)
            let sample_values: Vec<String> = values
                .iter()
                .filter(|v| !v.trim().is_empty())
                .take(10)
                .cloned()
                .collect();

            // Calculate average length for strings
            let avg_length = if !values.is_empty() {
                let total_len: usize = values.iter().map(|v| v.len()).sum();
                Some(total_len as f64 / values.len() as f64)
            } else {
                None
            };

            // Map CSV field type to SQL data type
            let data_type = Self::field_type_to_sql_type(&field_type);

            // Detect semantic type
            let (semantic_type, patterns) = Self::detect_semantic_type(&field_type, values);

            let column = DiscoveredColumn {
                name: Self::sanitize_column_name(header),
                data_type,
                nullable,
                primary_key: false, // Cannot infer PK from CSV
                semantic_type,
                confidence: type_confidence,
                patterns,
                statistics: ColumnStatistics {
                    distinct_count,
                    null_fraction,
                    sample_count: row_count,
                    most_common_values: None, // Would require full pass
                    avg_length,
                    min_value: values.iter().min().cloned(),
                    max_value: values.iter().max().cloned(),
                },
                sample_values,
            };

            columns.push(column);
        }

        Ok(DiscoveredTable {
            name: table_name.to_uppercase(),
            columns,
            row_count: Some(row_count as u64),
        })
    }

    /// Map inferred field type to SQL data type
    fn field_type_to_sql_type(
        field_type: &crate::common::csv_utils::analysis::InferredFieldType,
    ) -> String {
        use crate::common::csv_utils::analysis::InferredFieldType;

        match field_type {
            InferredFieldType::Integer => "INTEGER".to_string(),
            InferredFieldType::Float => "DECIMAL".to_string(),
            InferredFieldType::Boolean => "BOOLEAN".to_string(),
            InferredFieldType::Date => "DATE".to_string(),
            InferredFieldType::Timestamp => "TIMESTAMP".to_string(),
            InferredFieldType::String
            | InferredFieldType::Email
            | InferredFieldType::Phone
            | InferredFieldType::Ssn
            | InferredFieldType::Url
            | InferredFieldType::IpAddress
            | InferredFieldType::CreditCard
            | InferredFieldType::Json
            | InferredFieldType::Xml => "VARCHAR".to_string(),
        }
    }

    /// Detect semantic type and patterns from inferred field type
    fn detect_semantic_type(
        field_type: &crate::common::csv_utils::analysis::InferredFieldType,
        values: &[String],
    ) -> (Option<String>, Vec<DetectedPattern>) {
        use crate::common::csv_utils::analysis::InferredFieldType;

        let semantic_type = match field_type {
            InferredFieldType::Email => Some("email".to_string()),
            InferredFieldType::Phone => Some("phone".to_string()),
            InferredFieldType::Ssn => Some("ssn".to_string()),
            InferredFieldType::Url => Some("url".to_string()),
            InferredFieldType::IpAddress => Some("ip_address".to_string()),
            InferredFieldType::CreditCard => Some("credit_card".to_string()),
            InferredFieldType::Json => Some("json".to_string()),
            InferredFieldType::Xml => Some("xml".to_string()),
            _ => None,
        };

        let patterns = if let Some(sem_type) = &semantic_type {
            vec![DetectedPattern {
                pattern_type: sem_type.clone(),
                match_rate: 1.0, // High confidence from type inference
                example: values.first().cloned(),
            }]
        } else {
            vec![]
        };

        (semantic_type, patterns)
    }

    /// Sanitize column name for SQL
    fn sanitize_column_name(name: &str) -> String {
        name.trim()
            .replace(' ', "_")
            .replace('-', "_")
            .replace('.', "_")
            .replace('/', "_")
            .replace('\\', "_")
            .to_uppercase()
    }
}

#[async_trait]
impl SchemaExtractor for CsvExtractor {
    async fn extract_metadata(
        &self,
        source: &DataSource,
        _credentials: &Credentials,
        _schema_filter: Option<&str>,
        table_filter: Option<&str>,
    ) -> Result<SchemaMetadata> {
        // Extract file path from connection config
        use graphica_core::catalog::types::SourceConfig;

        let csv_path_str = match &source.connection.config {
            SourceConfig::CsvFile(config) => &config.path,
            _ => anyhow::bail!("Expected CsvFile config, got {:?}", source.source_type),
        };

        let csv_path = Path::new(csv_path_str);

        // Table name defaults to filename without extension
        let table_name = table_filter.unwrap_or_else(|| {
            csv_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("UNKNOWN")
        });

        // Extract schema from CSV
        let table = self.extract_from_path(csv_path, table_name, 1000).await?;

        // Convert to metadata format
        let columns = table
            .columns
            .iter()
            .map(|col| ColumnMetadata {
                name: col.name.clone(),
                data_type: col.data_type.clone(),
                nullable: col.nullable,
                default_value: None,
                primary_key: col.primary_key,
            })
            .collect();

        let table_metadata = TableMetadata {
            name: table.name.clone(),
            columns,
            estimated_rows: table.row_count,
        };

        Ok(SchemaMetadata {
            schema_name: "csv".to_string(), // CSV files don't have schema concept
            tables: vec![table_metadata],
            relationships: vec![],
        })
    }

    async fn extract_samples(
        &self,
        source: &DataSource,
        _credentials: &Credentials,
        _table_name: &str,
        sample_size: usize,
    ) -> Result<Vec<SampleRow>> {
        use csv::Reader;
        use graphica_core::catalog::types::SourceConfig;
        use std::collections::HashMap;

        let csv_path_str = match &source.connection.config {
            SourceConfig::CsvFile(config) => &config.path,
            _ => anyhow::bail!("Expected CsvFile config, got {:?}", source.source_type),
        };

        let csv_path = Path::new(csv_path_str);
        let mut reader = Reader::from_path(csv_path)
            .with_context(|| format!("Failed to open CSV file: {}", csv_path.display()))?;

        let headers = reader.headers()?.clone();
        let mut samples = Vec::new();

        for result in reader.records().take(sample_size) {
            let record = result?;
            let mut values = HashMap::new();

            for (col_idx, header) in headers.iter().enumerate() {
                if let Some(value) = record.get(col_idx) {
                    values.insert(header.to_string(), value.to_string());
                }
            }

            samples.push(SampleRow { values });
        }

        Ok(samples)
    }

    async fn extract_statistics(
        &self,
        source: &DataSource,
        _credentials: &Credentials,
        _table_name: &str,
        column_name: &str,
    ) -> Result<ColumnStats> {
        use csv::Reader;
        use graphica_core::catalog::types::SourceConfig;
        use std::collections::HashSet;

        let csv_path_str = match &source.connection.config {
            SourceConfig::CsvFile(config) => &config.path,
            _ => anyhow::bail!("Expected CsvFile config, got {:?}", source.source_type),
        };

        let csv_path = Path::new(csv_path_str);
        let mut reader = Reader::from_path(csv_path)?;

        let headers = reader.headers()?;
        let col_idx = headers
            .iter()
            .position(|h| h == column_name)
            .ok_or_else(|| anyhow::anyhow!("Column not found: {}", column_name))?;

        let mut distinct_values = HashSet::new();
        let mut null_count = 0;
        let mut total_count = 0;

        for result in reader.records().take(10000) {
            // Limit for performance
            let record = result?;
            total_count += 1;

            if let Some(value) = record.get(col_idx) {
                if value.trim().is_empty() {
                    null_count += 1;
                } else {
                    distinct_values.insert(value.to_string());
                }
            }
        }

        let null_fraction = if total_count > 0 {
            null_count as f64 / total_count as f64
        } else {
            0.0
        };

        Ok(ColumnStats {
            distinct_count: distinct_values.len() as i64,
            null_fraction,
            most_common_values: None,
        })
    }

    fn name(&self) -> &'static str {
        "csv"
    }

    fn supports_source(&self, source_type: &str) -> bool {
        source_type == "csv" || source_type == "file"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_column_name() {
        assert_eq!(
            CsvExtractor::sanitize_column_name("First Name"),
            "FIRST_NAME"
        );
        assert_eq!(
            CsvExtractor::sanitize_column_name("customer-id"),
            "CUSTOMER_ID"
        );
        assert_eq!(
            CsvExtractor::sanitize_column_name("email.address"),
            "EMAIL_ADDRESS"
        );
    }
}
