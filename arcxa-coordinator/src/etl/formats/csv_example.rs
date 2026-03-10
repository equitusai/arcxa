//! Example CSV Format Reader Implementation
//!
//! This demonstrates how to implement the FormatReader trait for CSV files,
//! showing the separation between format handling and destination logic.

use crate::etl::traits::{
    DataRecord, DataType, FieldSchema, FormatCapabilities, FormatReader, FormatStats,
    RecordSchema, SourceLocation, ValidationReport, ValidationError, EtlError,
};
use async_trait::async_trait;
use csv_async::{AsyncReader, AsyncReaderBuilder};
use futures::stream::{self, Stream, StreamExt};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use tokio::fs::File;
use tokio::io::BufReader;

/// CSV format reader implementation
pub struct CsvReader {
    path: PathBuf,
    delimiter: u8,
    has_header: bool,
    batch_size: usize,
    encoding: String,
    quote_char: Option<u8>,
    escape_char: Option<u8>,
    skip_lines: usize,
}

impl CsvReader {
    /// Create a new CSV reader
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            delimiter: b',',
            has_header: true,
            batch_size: 1000,
            encoding: "utf-8".to_string(),
            quote_char: Some(b'"'),
            escape_char: Some(b'\\'),
            skip_lines: 0,
        }
    }

    /// Builder pattern for configuration
    pub fn with_delimiter(mut self, delimiter: u8) -> Self {
        self.delimiter = delimiter;
        self
    }

    pub fn with_header(mut self, has_header: bool) -> Self {
        self.has_header = has_header;
        self
    }

    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }

    pub fn with_skip_lines(mut self, skip_lines: usize) -> Self {
        self.skip_lines = skip_lines;
        self
    }

    /// Create CSV reader with proper configuration
    async fn create_reader(&self) -> Result<AsyncReader<BufReader<File>>, EtlError> {
        let file = File::open(&self.path)
            .await
            .map_err(|e| EtlError::IoError {
                path: self.path.display().to_string(),
                error: e.to_string(),
            })?;

        let mut builder = AsyncReaderBuilder::new();
        builder
            .delimiter(self.delimiter)
            .has_headers(self.has_header);

        if let Some(quote) = self.quote_char {
            builder.quote(quote);
        }

        if let Some(escape) = self.escape_char {
            builder.escape(Some(escape));
        }

        let reader = builder.create_reader(BufReader::new(file));
        Ok(reader)
    }

    /// Infer data types from sample records
    fn infer_field_type(values: &[Option<String>]) -> DataType {
        // Simple type inference logic
        let non_null_values: Vec<_> = values.iter().filter_map(|v| v.as_ref()).collect();

        if non_null_values.is_empty() {
            return DataType::String;
        }

        // Try parsing as different types
        let all_integers = non_null_values.iter().all(|v| v.parse::<i64>().is_ok());
        if all_integers {
            return DataType::Integer;
        }

        let all_floats = non_null_values.iter().all(|v| v.parse::<f64>().is_ok());
        if all_floats {
            return DataType::Double;
        }

        let all_bools = non_null_values.iter().all(|v| {
            matches!(v.to_lowercase().as_str(), "true" | "false" | "t" | "f" | "0" | "1")
        });
        if all_bools {
            return DataType::Boolean;
        }

        // Check for date patterns (simple check)
        let all_dates = non_null_values.iter().all(|v| {
            v.contains('-') && v.len() == 10 // Simple YYYY-MM-DD check
        });
        if all_dates {
            return DataType::Date;
        }

        DataType::String
    }
}

#[async_trait]
impl FormatReader for CsvReader {
    async fn read_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<DataRecord, EtlError>> + Send>>, EtlError> {
        let mut reader = self.create_reader().await?;

        // Get headers
        let headers = if self.has_header {
            reader
                .headers()
                .await
                .map_err(|e| EtlError::ParseError {
                    line: 0,
                    message: format!("Failed to read headers: {}", e),
                })?
                .iter()
                .map(|h| h.to_string())
                .collect::<Vec<_>>()
        } else {
            // Generate column names if no header
            let mut headers = Vec::new();
            for i in 0..100 {
                // Assume max 100 columns
                headers.push(format!("column_{}", i));
            }
            headers
        };

        // Skip lines if configured
        for _ in 0..self.skip_lines {
            let mut record = csv_async::StringRecord::new();
            if !reader.read_record(&mut record).await.unwrap_or(false) {
                break;
            }
        }

        // Create stream of records
        let path = self.path.clone();
        let batch_size = self.batch_size;

        let stream = stream::unfold(
            (reader, headers, 0usize),
            move |(mut reader, headers, line_num)| {
                let path = path.clone();
                async move {
                    let mut record = csv_async::StringRecord::new();

                    match reader.read_record(&mut record).await {
                        Ok(true) => {
                            // Convert CSV record to DataRecord
                            let mut data = serde_json::Map::new();

                            for (i, field) in record.iter().enumerate() {
                                if i < headers.len() {
                                    let value = if field.is_empty() {
                                        Value::Null
                                    } else {
                                        Value::String(field.to_string())
                                    };
                                    data.insert(headers[i].clone(), value);
                                }
                            }

                            let data_record = DataRecord {
                                data: Value::Object(data),
                                schema: None,
                                source_location: Some(SourceLocation {
                                    file: Some(path.display().to_string()),
                                    line: Some(line_num as u64 + 1),
                                    byte_offset: None,
                                    partition: None,
                                }),
                                metadata: HashMap::new(),
                            };

                            Some((Ok(data_record), (reader, headers, line_num + 1)))
                        }
                        Ok(false) => None, // End of file
                        Err(e) => {
                            let error = EtlError::ParseError {
                                line: line_num,
                                message: format!("CSV parse error: {}", e),
                            };
                            Some((Err(error), (reader, headers, line_num + 1)))
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }

    async fn infer_schema(&self) -> Result<RecordSchema, EtlError> {
        let mut reader = self.create_reader().await?;

        // Get headers
        let headers = if self.has_header {
            reader
                .headers()
                .await
                .map_err(|e| EtlError::ParseError {
                    line: 0,
                    message: format!("Failed to read headers: {}", e),
                })?
                .iter()
                .map(|h| h.to_string())
                .collect::<Vec<_>>()
        } else {
            return Err(EtlError::SchemaError {
                message: "Cannot infer schema without headers".to_string(),
            });
        };

        // Read sample records for type inference
        let mut sample_data: Vec<Vec<Option<String>>> = vec![vec![]; headers.len()];
        let mut record = csv_async::StringRecord::new();
        let sample_size = 100;

        for _ in 0..sample_size {
            if !reader.read_record(&mut record).await.unwrap_or(false) {
                break;
            }

            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    let value = if field.is_empty() {
                        None
                    } else {
                        Some(field.to_string())
                    };
                    sample_data[i].push(value);
                }
            }
        }

        // Infer types from samples
        let fields: Vec<FieldSchema> = headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let data_type = Self::infer_field_type(&sample_data[i]);
                let nullable = sample_data[i].iter().any(|v| v.is_none());

                FieldSchema {
                    name: name.clone(),
                    data_type,
                    nullable,
                    description: None,
                    metadata: HashMap::new(),
                }
            })
            .collect();

        Ok(RecordSchema {
            fields,
            metadata: HashMap::new(),
        })
    }

    async fn get_stats(&self) -> Result<FormatStats, EtlError> {
        let file_metadata = tokio::fs::metadata(&self.path)
            .await
            .map_err(|e| EtlError::IoError {
                path: self.path.display().to_string(),
                error: e.to_string(),
            })?;

        // Quick line count (approximate)
        let file = File::open(&self.path).await.map_err(|e| EtlError::IoError {
            path: self.path.display().to_string(),
            error: e.to_string(),
        })?;

        let reader = BufReader::new(file);
        use tokio::io::AsyncBufReadExt;
        let mut lines = tokio::io::BufReader::new(reader).lines();
        let mut line_count = 0u64;

        while lines.next_line().await.map_err(|e| EtlError::IoError {
            path: self.path.display().to_string(),
            error: e.to_string(),
        })?.is_some() {
            line_count += 1;
        }

        let record_count = if self.has_header && line_count > 0 {
            line_count - 1
        } else {
            line_count
        };

        let mut metadata = HashMap::new();
        metadata.insert("delimiter".to_string(), json!(self.delimiter as char));
        metadata.insert("has_header".to_string(), json!(self.has_header));
        metadata.insert("encoding".to_string(), json!(self.encoding));
        metadata.insert("skip_lines".to_string(), json!(self.skip_lines));

        Ok(FormatStats {
            total_records: Some(record_count),
            total_bytes: Some(file_metadata.len()),
            format_name: "CSV".to_string(),
            compression: None,
            metadata,
        })
    }

    async fn validate(&self) -> Result<ValidationReport, EtlError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Check file exists
        if !self.path.exists() {
            errors.push(ValidationError {
                code: "FILE_NOT_FOUND".to_string(),
                message: format!("File not found: {}", self.path.display()),
                location: None,
            });
        }

        // Check file is readable
        if let Err(e) = File::open(&self.path).await {
            errors.push(ValidationError {
                code: "FILE_NOT_READABLE".to_string(),
                message: format!("Cannot read file: {}", e),
                location: None,
            });
        }

        // Try to parse first few lines
        if errors.is_empty() {
            match self.create_reader().await {
                Ok(mut reader) => {
                    let mut record = csv_async::StringRecord::new();
                    for line_num in 0..10 {
                        match reader.read_record(&mut record).await {
                            Ok(false) => break, // EOF
                            Ok(true) => {
                                // Check for consistent column count
                                if line_num == 0 && self.has_header {
                                    let header_count = record.len();
                                    if header_count == 0 {
                                        errors.push(ValidationError {
                                            code: "EMPTY_HEADER".to_string(),
                                            message: "Header row is empty".to_string(),
                                            location: Some(SourceLocation {
                                                file: Some(self.path.display().to_string()),
                                                line: Some(1),
                                                byte_offset: None,
                                                partition: None,
                                            }),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                errors.push(ValidationError {
                                    code: "PARSE_ERROR".to_string(),
                                    message: format!("Parse error at line {}: {}", line_num + 1, e),
                                    location: Some(SourceLocation {
                                        file: Some(self.path.display().to_string()),
                                        line: Some(line_num as u64 + 1),
                                        byte_offset: None,
                                        partition: None,
                                    }),
                                });
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    errors.push(ValidationError {
                        code: "READER_CREATION_FAILED".to_string(),
                        message: format!("Failed to create CSV reader: {}", e),
                        location: None,
                    });
                }
            }
        }

        Ok(ValidationReport {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        })
    }

    fn capabilities(&self) -> FormatCapabilities {
        FormatCapabilities {
            supports_schema_inference: true,
            supports_partitioning: false,
            supports_pushdown_filters: false,
            supports_projection: false,
            is_streaming: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn test_csv_reader_with_header() {
        // Create a temporary CSV file
        let temp_dir = tempfile::tempdir().unwrap();
        let csv_path = temp_dir.path().join("test.csv");

        let mut file = tokio::fs::File::create(&csv_path).await.unwrap();
        file.write_all(b"id,name,age\n1,Alice,30\n2,Bob,25\n")
            .await
            .unwrap();
        file.flush().await.unwrap();

        // Create reader and test
        let reader = CsvReader::new(csv_path);

        // Test schema inference
        let schema = reader.infer_schema().await.unwrap();
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields[0].name, "id");
        assert_eq!(schema.fields[0].data_type, DataType::Integer);
        assert_eq!(schema.fields[1].name, "name");
        assert_eq!(schema.fields[1].data_type, DataType::String);
        assert_eq!(schema.fields[2].name, "age");
        assert_eq!(schema.fields[2].data_type, DataType::Integer);

        // Test reading records
        let mut stream = reader.read_stream().await.unwrap();
        let mut records = Vec::new();

        while let Some(result) = stream.next().await {
            records.push(result.unwrap());
        }

        assert_eq!(records.len(), 2);

        // Check first record
        let first = &records[0];
        assert_eq!(first.data["id"], json!("1"));
        assert_eq!(first.data["name"], json!("Alice"));
        assert_eq!(first.data["age"], json!("30"));

        // Test stats
        let stats = reader.get_stats().await.unwrap();
        assert_eq!(stats.total_records, Some(2));
        assert_eq!(stats.format_name, "CSV");
    }
}