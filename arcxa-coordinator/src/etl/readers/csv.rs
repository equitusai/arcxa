//! CSV Format Reader
//!
//! Production-grade CSV reader implementing the FormatReader trait.
//! Integrates with the File Library First architecture.

use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::etl::errors::EtlError;
use crate::etl::traits::*;
use async_trait::async_trait;
use futures::stream::{self, Stream};
use std::collections::HashMap;
use std::io::Cursor;
use std::pin::Pin;
use std::sync::Arc;

/// CSV reader implementing FormatReader trait
///
/// # File Library First
/// This reader MUST use FileLibraryStore instead of direct file paths.
/// All CSV files must be registered in the file library before processing.
///
/// # Example
/// ```no_run
/// use graphica_coordinator::etl::readers::{CsvReader, CsvOptions};
/// use graphica_coordinator::etl::FormatReader;
/// use graphica_coordinator::api::file_library::storage_trait::FileLibraryStore;
/// use std::sync::Arc;
///
/// # async fn example(file_store: Arc<dyn FileLibraryStore>) {
/// let reader = CsvReader::new(
///     file_store,
///     "file_123".to_string(),
///     CsvOptions::default(),
/// );
///
/// let stream = reader.read_stream().await.unwrap();
/// # }
/// ```
pub struct CsvReader {
    file_store: Arc<dyn FileLibraryStore>,
    file_id: String,
    options: CsvOptions,
}

/// CSV parsing options
#[derive(Debug, Clone)]
pub struct CsvOptions {
    /// Field delimiter (default: ',')
    pub delimiter: u8,

    /// Whether the file has a header row (default: true)
    pub has_header: bool,

    /// Quote character (default: '"')
    pub quote_char: u8,

    /// Escape character (default: None, uses double-quote escaping)
    pub escape_char: Option<u8>,

    /// Number of rows to skip at the beginning (default: 0)
    pub skip_rows: usize,

    /// Expected number of columns (for validation, None = infer from first row)
    pub expected_columns: Option<usize>,

    /// Whether to trim whitespace from fields (default: false)
    pub trim_whitespace: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: b',',
            has_header: true,
            quote_char: b'"',
            escape_char: None,
            skip_rows: 0,
            expected_columns: None,
            trim_whitespace: false,
        }
    }
}

impl CsvOptions {
    /// Create options for tab-separated files
    pub fn tab_separated() -> Self {
        Self {
            delimiter: b'\t',
            ..Default::default()
        }
    }

    /// Create options for pipe-separated files
    pub fn pipe_separated() -> Self {
        Self {
            delimiter: b'|',
            ..Default::default()
        }
    }

    /// Create options for semicolon-separated files
    pub fn semicolon_separated() -> Self {
        Self {
            delimiter: b';',
            ..Default::default()
        }
    }
}

impl CsvReader {
    /// Create a new CSV reader
    ///
    /// # Arguments
    /// * `file_store` - File library storage backend
    /// * `file_id` - File ID registered in the file library
    /// * `options` - CSV parsing options
    pub fn new(
        file_store: Arc<dyn FileLibraryStore>,
        file_id: String,
        options: CsvOptions,
    ) -> Self {
        Self {
            file_store,
            file_id,
            options,
        }
    }

    /// Get file content from file library
    async fn get_file_content(&self) -> Result<Vec<u8>, EtlError> {
        // Get file metadata
        let file = self
            .file_store
            .get_file(&self.file_id)
            .map_err(|e| EtlError::IoError {
                path: self.file_id.clone(),
                error: format!("Failed to get file from library: {}", e),
            })?
            .ok_or_else(|| EtlError::IoError {
                path: self.file_id.clone(),
                error: "File not found in library".to_string(),
            })?;

        // Read file content from disk
        tokio::fs::read(&file.file_path)
            .await
            .map_err(|e| EtlError::IoError {
                path: file.file_path.clone(),
                error: format!("Failed to read file content: {}", e),
            })
    }

    /// Build CSV reader from bytes
    fn build_csv_reader(&self, content: &[u8]) -> csv::Reader<Cursor<Vec<u8>>> {
        let mut builder = csv::ReaderBuilder::new();
        builder
            .delimiter(self.options.delimiter)
            .has_headers(self.options.has_header)
            .quote(self.options.quote_char)
            .trim(if self.options.trim_whitespace {
                csv::Trim::All
            } else {
                csv::Trim::None
            });

        if let Some(escape) = self.options.escape_char {
            builder.escape(Some(escape));
        }

        builder.from_reader(Cursor::new(content.to_vec()))
    }

    /// Get header names (either from file or generated)
    fn get_headers(
        &self,
        reader: &mut csv::Reader<Cursor<Vec<u8>>>,
    ) -> Result<Vec<String>, EtlError> {
        if self.options.has_header {
            let headers = reader.headers().map_err(|e| EtlError::FormatError {
                format: "CSV".to_string(),
                message: format!("Failed to read headers: {}", e),
                source: None,
            })?;

            Ok(headers.iter().map(|s| s.to_string()).collect())
        } else {
            // Generate default column names
            let first_record =
                reader
                    .records()
                    .next()
                    .transpose()
                    .map_err(|e| EtlError::FormatError {
                        format: "CSV".to_string(),
                        message: format!("Failed to read first record: {}", e),
                        source: None,
                    })?;

            if let Some(record) = first_record {
                Ok((0..record.len()).map(|i| format!("column_{}", i)).collect())
            } else {
                Ok(vec![])
            }
        }
    }

    /// Infer data type from a set of sample values
    fn infer_type(&self, values: &[String]) -> DataType {
        if values.is_empty() {
            return DataType::String;
        }

        let mut all_integers = true;
        let mut all_floats = true;
        let mut all_booleans = true;
        let mut all_dates = true;

        for value in values {
            let trimmed = value.trim();

            if trimmed.is_empty() {
                continue; // Skip empty values
            }

            // Check integer
            if all_integers && trimmed.parse::<i64>().is_err() {
                all_integers = false;
            }

            // Check float
            if all_floats && trimmed.parse::<f64>().is_err() {
                all_floats = false;
            }

            // Check boolean
            if all_booleans {
                let lower = trimmed.to_lowercase();
                if lower != "true"
                    && lower != "false"
                    && lower != "t"
                    && lower != "f"
                    && lower != "yes"
                    && lower != "no"
                    && lower != "y"
                    && lower != "n"
                    && lower != "1"
                    && lower != "0"
                {
                    all_booleans = false;
                }
            }

            // Check date (ISO 8601 format: YYYY-MM-DD)
            if all_dates {
                if trimmed.len() != 10
                    || !trimmed.chars().nth(4).map_or(false, |c| c == '-')
                    || !trimmed.chars().nth(7).map_or(false, |c| c == '-')
                {
                    all_dates = false;
                } else if let Err(_) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
                    all_dates = false;
                }
            }
        }

        // Return most specific type
        if all_integers {
            DataType::BigInt
        } else if all_floats {
            DataType::Double
        } else if all_booleans {
            DataType::Boolean
        } else if all_dates {
            DataType::Date
        } else {
            DataType::String
        }
    }
}

#[async_trait]
impl FormatReader for CsvReader {
    async fn read_stream(
        &self,
    ) -> Result<Pin<Box<dyn Stream<Item = anyhow::Result<DataRecord>> + Send>>, EtlError> {
        let content = self.get_file_content().await?;

        // Handle empty files
        if content.is_empty() {
            return Ok(Box::pin(stream::empty()));
        }

        // Strip UTF-8 BOM if present
        let content = if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &content[3..]
        } else {
            &content
        };

        let mut reader = self.build_csv_reader(content);

        // Get headers
        let headers = self.get_headers(&mut reader)?;

        if headers.is_empty() {
            return Ok(Box::pin(stream::empty()));
        }

        // Rebuild reader to start from beginning (get_headers may have consumed first row)
        let mut reader = self.build_csv_reader(content);

        // Convert all records to DataRecord
        let file_id = self.file_id.clone();
        let expected_columns = self.options.expected_columns.or(Some(headers.len()));
        let mut records = Vec::new();
        let mut line_number = if self.options.has_header { 2 } else { 1 };
        let mut rows_to_skip = self.options.skip_rows;

        for result in reader.records() {
            // Skip rows if requested
            if rows_to_skip > 0 {
                rows_to_skip -= 1;
                line_number += 1;
                continue;
            }
            let record = result.map_err(|e| EtlError::ParseError {
                line: line_number,
                message: format!("CSV parse error: {}", e),
            })?;

            // Validate column count
            if let Some(expected) = expected_columns {
                if record.len() != expected {
                    return Err(EtlError::ParseError {
                        line: line_number,
                        message: format!(
                            "Column count mismatch: expected {}, got {}",
                            expected,
                            record.len()
                        ),
                    });
                }
            }

            // Build JSON object
            let mut data = serde_json::Map::new();
            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    data.insert(
                        headers[i].clone(),
                        serde_json::Value::String(field.to_string()),
                    );
                }
            }

            records.push(Ok::<DataRecord, anyhow::Error>(DataRecord {
                data: serde_json::Value::Object(data),
                schema: None,
                source_location: Some(SourceLocation {
                    file: Some(file_id.clone()),
                    line: Some(line_number as u64),
                    byte_offset: None,
                    partition: None,
                }),
                metadata: HashMap::new(),
            }));

            line_number += 1;
        }

        Ok(Box::pin(stream::iter(records)))
    }

    async fn infer_schema(&self) -> Result<RecordSchema, EtlError> {
        let content = self.get_file_content().await?;

        if content.is_empty() {
            return Ok(RecordSchema {
                fields: vec![],
                metadata: HashMap::new(),
            });
        }

        // Strip UTF-8 BOM if present
        let content = if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &content[3..]
        } else {
            &content
        };

        let mut reader = self.build_csv_reader(content);
        let headers = self.get_headers(&mut reader)?;

        if headers.is_empty() {
            return Ok(RecordSchema {
                fields: vec![],
                metadata: HashMap::new(),
            });
        }

        // Collect sample values for type inference (up to 100 rows)
        let mut field_samples: HashMap<String, Vec<String>> = HashMap::new();
        for header in &headers {
            field_samples.insert(header.clone(), Vec::new());
        }

        let mut count = 0;
        for result in reader.records() {
            if count >= 100 {
                break;
            }

            let record = result.map_err(|e| EtlError::ParseError {
                line: count + if self.options.has_header { 2 } else { 1 },
                message: format!("Failed to read record for schema inference: {}", e),
            })?;

            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    field_samples
                        .get_mut(&headers[i])
                        .unwrap()
                        .push(field.to_string());
                }
            }

            count += 1;
        }

        // Infer types for each field
        let mut fields = Vec::new();
        for header in &headers {
            let samples = field_samples.get(header).unwrap();
            let data_type = self.infer_type(samples);

            // Check if field has any null/empty values
            let nullable = samples.iter().any(|s| s.trim().is_empty());

            fields.push(FieldSchema {
                name: header.clone(),
                data_type,
                nullable,
                description: None,
                metadata: HashMap::new(),
            });
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "sample_size".to_string(),
            serde_json::Value::Number(count.into()),
        );

        Ok(RecordSchema { fields, metadata })
    }

    async fn get_stats(&self) -> Result<FormatStats, EtlError> {
        let content = self.get_file_content().await?;
        let content_len = content.len();

        if content.is_empty() {
            return Ok(FormatStats {
                total_records: Some(0),
                total_bytes: Some(0),
                format_name: "CSV".to_string(),
                compression: None,
                metadata: HashMap::new(),
            });
        }

        // Strip UTF-8 BOM if present
        let content = if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &content[3..]
        } else {
            &content
        };

        let mut reader = self.build_csv_reader(content);

        // Count records
        let mut record_count = 0u64;
        for result in reader.records() {
            result.map_err(|e| EtlError::FormatError {
                format: "CSV".to_string(),
                message: format!("Failed to count records: {}", e),
                source: None,
            })?;
            record_count += 1;
        }

        let mut metadata = HashMap::new();
        metadata.insert(
            "has_header".to_string(),
            serde_json::Value::Bool(self.options.has_header),
        );
        metadata.insert(
            "delimiter".to_string(),
            serde_json::Value::String(format!("{}", self.options.delimiter as char)),
        );

        Ok(FormatStats {
            total_records: Some(record_count),
            total_bytes: Some(content_len as u64),
            format_name: "CSV".to_string(),
            compression: None,
            metadata,
        })
    }

    async fn validate(&self) -> Result<ValidationReport, EtlError> {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        let content = match self.get_file_content().await {
            Ok(c) => c,
            Err(e) => {
                errors.push(ValidationError {
                    code: "FILE_READ_ERROR".to_string(),
                    message: format!("Cannot read file: {}", e),
                    location: None,
                });
                return Ok(ValidationReport {
                    is_valid: false,
                    errors,
                    warnings,
                });
            }
        };

        if content.is_empty() {
            warnings.push(ValidationWarning {
                code: "EMPTY_FILE".to_string(),
                message: "File is empty".to_string(),
                location: None,
            });
            return Ok(ValidationReport {
                is_valid: true,
                errors,
                warnings,
            });
        }

        // Strip UTF-8 BOM if present
        let content = if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
            &content[3..]
        } else {
            &content
        };

        let mut reader = self.build_csv_reader(content);

        // Try to read headers
        let headers = match self.get_headers(&mut reader) {
            Ok(h) => h,
            Err(e) => {
                errors.push(ValidationError {
                    code: "INVALID_HEADERS".to_string(),
                    message: format!("Cannot read headers: {}", e),
                    location: None,
                });
                return Ok(ValidationReport {
                    is_valid: false,
                    errors,
                    warnings,
                });
            }
        };

        if headers.is_empty() {
            warnings.push(ValidationWarning {
                code: "NO_HEADERS".to_string(),
                message: "File has no headers or columns".to_string(),
                location: None,
            });
        }

        // Check for duplicate headers
        let mut seen_headers = std::collections::HashSet::new();
        for (i, header) in headers.iter().enumerate() {
            if !seen_headers.insert(header) {
                warnings.push(ValidationWarning {
                    code: "DUPLICATE_HEADER".to_string(),
                    message: format!("Duplicate header '{}' at column {}", header, i),
                    location: None,
                });
            }
        }

        // Validate all records
        let expected_columns = headers.len();
        let mut line_number = if self.options.has_header { 2 } else { 1 };
        let mut record_count = 0;

        for result in reader.records() {
            match result {
                Ok(record) => {
                    // Check column count consistency
                    if record.len() != expected_columns {
                        errors.push(ValidationError {
                            code: "COLUMN_COUNT_MISMATCH".to_string(),
                            message: format!(
                                "Expected {} columns, found {}",
                                expected_columns,
                                record.len()
                            ),
                            location: Some(SourceLocation {
                                file: Some(self.file_id.clone()),
                                line: Some(line_number as u64),
                                byte_offset: None,
                                partition: None,
                            }),
                        });
                    }

                    // Check for very long fields (>10KB)
                    for (i, field) in record.iter().enumerate() {
                        if field.len() > 10240 {
                            warnings.push(ValidationWarning {
                                code: "LARGE_FIELD".to_string(),
                                message: format!(
                                    "Field '{}' at line {} exceeds 10KB ({} bytes)",
                                    headers.get(i).unwrap_or(&format!("column_{}", i)),
                                    line_number,
                                    field.len()
                                ),
                                location: Some(SourceLocation {
                                    file: Some(self.file_id.clone()),
                                    line: Some(line_number as u64),
                                    byte_offset: None,
                                    partition: None,
                                }),
                            });
                        }
                    }

                    record_count += 1;
                }
                Err(e) => {
                    errors.push(ValidationError {
                        code: "PARSE_ERROR".to_string(),
                        message: format!("Parse error: {}", e),
                        location: Some(SourceLocation {
                            file: Some(self.file_id.clone()),
                            line: Some(line_number as u64),
                            byte_offset: None,
                            partition: None,
                        }),
                    });
                }
            }

            line_number += 1;
        }

        if record_count == 0 && self.options.has_header {
            warnings.push(ValidationWarning {
                code: "NO_DATA_ROWS".to_string(),
                message: "File has headers but no data rows".to_string(),
                location: None,
            });
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
    use crate::api::file_library::types::*;
    use chrono::Utc;
    use std::collections::HashMap;

    /// Mock file store for testing
    struct MockFileStore {
        files: HashMap<String, (DataFile, Vec<u8>)>,
    }

    impl MockFileStore {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
            }
        }

        fn add_file(&mut self, file_id: &str, file_path: &str, content: Vec<u8>) {
            let file = DataFile {
                id: file_id.to_string(),
                name: format!("{}.csv", file_id),
                file_path: file_path.to_string(),
                folder_id: None,
                description: None,
                owner: FileOwner {
                    user_id: "test_user".to_string(),
                    email: "test@example.com".to_string(),
                    name: "Test User".to_string(),
                },
                size_bytes: content.len() as u64,
                encoding: "UTF-8".to_string(),
                delimiter: ",".to_string(),
                has_header: true,
                schema: None,
                ontology_mappings: vec![],
                status: FileStatus::Validated,
                validation_errors: vec![],
                validation_warnings: vec![],
                tags: vec![],
                metadata: HashMap::new(),
                sensitivity_level: None,
                retention_policy: None,
                access_control: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                last_accessed: None,
                version: None,
                previous_versions: vec![],
            };

            self.files.insert(file_id.to_string(), (file, content));
        }
    }

    impl FileLibraryStore for MockFileStore {
        fn create_file(&self, _file: DataFile) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_file(&self, file_id: &str) -> anyhow::Result<Option<DataFile>> {
            Ok(self.files.get(file_id).map(|(f, _)| f.clone()))
        }

        fn update_file(
            &self,
            _file_id: &str,
            _updates: UpdateFileRequest,
        ) -> anyhow::Result<DataFile> {
            unimplemented!()
        }

        fn delete_file(&self, _file_id: &str) -> anyhow::Result<()> {
            unimplemented!()
        }

        fn update_last_accessed(&self, _file_id: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn list_files(&self, _request: &ListFilesRequest) -> anyhow::Result<Vec<DataFile>> {
            Ok(vec![])
        }

        fn search_files(&self, _request: &SearchRequest) -> anyhow::Result<Vec<DataFile>> {
            Ok(vec![])
        }

        fn create_folder(&self, _folder: Folder) -> anyhow::Result<Folder> {
            unimplemented!()
        }

        fn get_folder(&self, _folder_id: &str) -> anyhow::Result<Option<Folder>> {
            Ok(None)
        }

        fn list_folders(&self) -> anyhow::Result<Vec<Folder>> {
            Ok(vec![])
        }

        fn update_folder(
            &self,
            _folder_id: &str,
            _updates: UpdateFolderRequest,
        ) -> anyhow::Result<Folder> {
            unimplemented!()
        }

        fn delete_folder(&self, _folder_id: &str, _force: bool) -> anyhow::Result<()> {
            unimplemented!()
        }

        fn create_job(&self, _job: ImportJob) -> anyhow::Result<()> {
            Ok(())
        }

        fn get_job(&self, _job_id: &str) -> anyhow::Result<Option<ImportJob>> {
            Ok(None)
        }

        fn update_job(&self, _job: ImportJob) -> anyhow::Result<()> {
            Ok(())
        }

        fn update_job_progress(
            &self,
            _job_id: &str,
            _processed_files: usize,
            _progress_percent: f32,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn complete_job(
            &self,
            _job_id: &str,
            _status: JobStatus,
            _successful_files: usize,
            _failed_files: usize,
            _results: Vec<ImportResult>,
            _duration_ms: u64,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        fn list_tags(&self) -> anyhow::Result<Vec<TagInfo>> {
            Ok(vec![])
        }

        fn get_statistics(&self) -> anyhow::Result<LibraryStatsResponse> {
            Ok(LibraryStatsResponse {
                total_files: 0,
                total_size_bytes: 0,
                total_rows: 0,
                files_by_status: HashMap::new(),
                files_by_folder: HashMap::new(),
                files_with_pii: 0,
                top_tags: vec![],
                recent_uploads: vec![],
                most_used: vec![],
            })
        }
    }

    async fn create_test_file(file_id: &str, content: &str) -> (Arc<MockFileStore>, String) {
        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join(format!("csv_test_{}.csv", file_id));
        tokio::fs::write(&file_path, content).await.unwrap();

        let mut store = MockFileStore::new();
        store.add_file(
            file_id,
            file_path.to_str().unwrap(),
            content.as_bytes().to_vec(),
        );

        (Arc::new(store), file_id.to_string())
    }

    #[tokio::test]
    async fn test_csv_with_headers() {
        let (store, file_id) = create_test_file(
            "test_headers",
            "name,age,email\nAlice,30,alice@example.com\nBob,25,bob@example.com\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let mut stream = reader.read_stream().await.unwrap();

        let mut count = 0;
        while let Some(result) = futures::StreamExt::next(&mut stream).await {
            let record = result.unwrap();
            assert!(record.data.is_object());
            count += 1;
        }

        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn test_csv_without_headers() {
        let (store, file_id) = create_test_file(
            "test_no_headers",
            "Alice,30,alice@example.com\nBob,25,bob@example.com\n",
        )
        .await;

        let mut options = CsvOptions::default();
        options.has_header = false;

        let reader = CsvReader::new(store, file_id, options);
        let schema = reader.infer_schema().await.unwrap();

        // Should generate column_0, column_1, column_2
        assert_eq!(schema.fields.len(), 3);
        assert_eq!(schema.fields[0].name, "column_0");
    }

    #[tokio::test]
    async fn test_custom_delimiter_pipe() {
        let (store, file_id) =
            create_test_file("test_pipe", "name|age|city\nAlice|30|NYC\nBob|25|LA\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::pipe_separated());
        let mut stream = reader.read_stream().await.unwrap();

        let record = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        let data = record.data.as_object().unwrap();
        assert_eq!(data.get("name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(data.get("city").unwrap().as_str().unwrap(), "NYC");
    }

    #[tokio::test]
    async fn test_custom_delimiter_tab() {
        let (store, file_id) =
            create_test_file("test_tab", "name\tage\tcity\nAlice\t30\tNYC\nBob\t25\tLA\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::tab_separated());
        let stats = reader.get_stats().await.unwrap();
        assert_eq!(stats.total_records, Some(2));
    }

    #[tokio::test]
    async fn test_custom_delimiter_semicolon() {
        let (store, file_id) =
            create_test_file("test_semicolon", "name;age;city\nAlice;30;NYC\nBob;25;LA\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::semicolon_separated());
        let schema = reader.infer_schema().await.unwrap();
        assert_eq!(schema.fields.len(), 3);
    }

    #[tokio::test]
    async fn test_empty_file() {
        let (store, file_id) = create_test_file("test_empty", "").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let mut stream = reader.read_stream().await.unwrap();

        let record = futures::StreamExt::next(&mut stream).await;
        assert!(record.is_none());
    }

    #[tokio::test]
    async fn test_single_row_file() {
        let (store, file_id) = create_test_file("test_single", "name,age\nAlice,30\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let stats = reader.get_stats().await.unwrap();
        assert_eq!(stats.total_records, Some(1));
    }

    #[tokio::test]
    async fn test_malformed_csv_unclosed_quote() {
        let (store, file_id) = create_test_file(
            "test_malformed",
            "name,age\n\"Alice,30\nBob,25\n", // Unclosed quote
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let result = reader.read_stream().await;

        // CSV parser should handle this - it might treat the rest as one field
        // or return an error
        match result {
            Ok(mut stream) => {
                // Try to consume the stream
                while let Some(result) = futures::StreamExt::next(&mut stream).await {
                    // Either succeeds or returns parse error
                    let _ = result;
                }
            }
            Err(_) => {
                // Parse error is also acceptable
            }
        }
    }

    #[tokio::test]
    async fn test_inconsistent_column_counts() {
        let (store, file_id) = create_test_file(
            "test_inconsistent",
            "name,age,city\nAlice,30,NYC\nBob,25\n", // Missing column
        )
        .await;

        let mut options = CsvOptions::default();
        options.expected_columns = Some(3);

        let reader = CsvReader::new(store, file_id, options);
        let result = reader.read_stream().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let store = Arc::new(MockFileStore::new());
        let reader = CsvReader::new(store, "nonexistent".to_string(), CsvOptions::default());

        let result = reader.read_stream().await;
        assert!(result.is_err());

        // Error contains "File not found"
        if let Err(err) = result {
            let err_string = err.to_string();
            assert!(err_string.contains("File not found") || err_string.contains("not found"));
        }
    }

    #[tokio::test]
    async fn test_quoted_fields_with_delimiters() {
        let (store, file_id) = create_test_file(
            "test_quoted",
            "name,description\nAlice,\"Hello, World\"\nBob,\"Test, data\"\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let mut stream = reader.read_stream().await.unwrap();

        let record = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        let data = record.data.as_object().unwrap();
        assert_eq!(
            data.get("description").unwrap().as_str().unwrap(),
            "Hello, World"
        );
    }

    #[tokio::test]
    async fn test_quoted_fields_with_newlines() {
        let (store, file_id) = create_test_file(
            "test_newlines",
            "name,address\nAlice,\"123 Main St\nApt 4B\"\nBob,\"456 Oak Ave\"\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let stats = reader.get_stats().await.unwrap();
        assert_eq!(stats.total_records, Some(2));
    }

    #[tokio::test]
    async fn test_escaped_quotes() {
        let (store, file_id) = create_test_file(
            "test_escaped",
            "name,quote\nAlice,\"She said, \"\"Hello\"\"\"\nBob,\"Normal text\"\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let mut stream = reader.read_stream().await.unwrap();

        let record = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        let data = record.data.as_object().unwrap();
        assert_eq!(
            data.get("quote").unwrap().as_str().unwrap(),
            "She said, \"Hello\""
        );
    }

    #[tokio::test]
    async fn test_mixed_types_in_column() {
        let (store, file_id) =
            create_test_file("test_mixed", "id,value\n1,123\n2,abc\n3,456\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let schema = reader.infer_schema().await.unwrap();

        // Mixed types should default to String
        let value_field = schema.fields.iter().find(|f| f.name == "value").unwrap();
        assert_eq!(value_field.data_type, DataType::String);
    }

    #[tokio::test]
    async fn test_very_long_lines() {
        let long_value = "x".repeat(15000); // 15KB
        let content = format!("name,data\nAlice,{}\nBob,short\n", long_value);

        let (store, file_id) = create_test_file("test_long", &content).await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let report = reader.validate().await.unwrap();

        // Should have warning about large field
        assert!(!report.warnings.is_empty());
        assert!(report.warnings.iter().any(|w| w.code == "LARGE_FIELD"));
    }

    #[tokio::test]
    async fn test_utf8_bom() {
        let content_with_bom = vec![0xEF, 0xBB, 0xBF]; // UTF-8 BOM
        let mut content = content_with_bom;
        content.extend_from_slice(b"name,age\nAlice,30\n");

        let temp_dir = std::env::temp_dir();
        let file_path = temp_dir.join("test_bom.csv");
        tokio::fs::write(&file_path, &content).await.unwrap();

        let mut store = MockFileStore::new();
        store.add_file("test_bom", file_path.to_str().unwrap(), content);

        let reader = CsvReader::new(
            Arc::new(store),
            "test_bom".to_string(),
            CsvOptions::default(),
        );
        let mut stream = reader.read_stream().await.unwrap();

        let record = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        let data = record.data.as_object().unwrap();
        assert!(data.contains_key("name"));
    }

    #[tokio::test]
    async fn test_schema_inference_integers() {
        let (store, file_id) =
            create_test_file("test_int", "id,count\n1,100\n2,200\n3,300\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let schema = reader.infer_schema().await.unwrap();

        let count_field = schema.fields.iter().find(|f| f.name == "count").unwrap();
        assert_eq!(count_field.data_type, DataType::BigInt);
    }

    #[tokio::test]
    async fn test_schema_inference_floats() {
        let (store, file_id) =
            create_test_file("test_float", "price,tax\n19.99,1.50\n29.99,2.25\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let schema = reader.infer_schema().await.unwrap();

        let price_field = schema.fields.iter().find(|f| f.name == "price").unwrap();
        assert_eq!(price_field.data_type, DataType::Double);
    }

    #[tokio::test]
    async fn test_schema_inference_dates() {
        let (store, file_id) = create_test_file(
            "test_date",
            "event,date\nBirthday,2024-01-15\nAnniversary,2024-06-20\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let schema = reader.infer_schema().await.unwrap();

        let date_field = schema.fields.iter().find(|f| f.name == "date").unwrap();
        assert_eq!(date_field.data_type, DataType::Date);
    }

    #[tokio::test]
    async fn test_schema_inference_booleans() {
        let (store, file_id) = create_test_file(
            "test_bool",
            "name,active\nAlice,true\nBob,false\nCharlie,true\n",
        )
        .await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let schema = reader.infer_schema().await.unwrap();

        let active_field = schema.fields.iter().find(|f| f.name == "active").unwrap();
        assert_eq!(active_field.data_type, DataType::Boolean);
    }

    #[tokio::test]
    async fn test_validation_duplicate_headers() {
        let (store, file_id) =
            create_test_file("test_dup_headers", "name,age,name\nAlice,30,duplicate\n").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let report = reader.validate().await.unwrap();

        assert!(report.warnings.iter().any(|w| w.code == "DUPLICATE_HEADER"));
    }

    #[tokio::test]
    async fn test_validation_empty_file_warning() {
        let (store, file_id) = create_test_file("test_empty_warn", "").await;

        let reader = CsvReader::new(store, file_id, CsvOptions::default());
        let report = reader.validate().await.unwrap();

        assert!(report.is_valid);
        assert!(report.warnings.iter().any(|w| w.code == "EMPTY_FILE"));
    }

    #[tokio::test]
    async fn test_source_location_tracking() {
        let (store, file_id) =
            create_test_file("test_location", "name,age\nAlice,30\nBob,25\n").await;

        let reader = CsvReader::new(store, file_id.clone(), CsvOptions::default());
        let mut stream = reader.read_stream().await.unwrap();

        let record1 = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record1.source_location.as_ref().unwrap().line, Some(2));
        assert_eq!(
            record1
                .source_location
                .as_ref()
                .unwrap()
                .file
                .as_ref()
                .unwrap(),
            &file_id
        );

        let record2 = futures::StreamExt::next(&mut stream)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(record2.source_location.as_ref().unwrap().line, Some(3));
    }

    #[tokio::test]
    async fn test_capabilities() {
        let store = Arc::new(MockFileStore::new());
        let reader = CsvReader::new(store, "test".to_string(), CsvOptions::default());

        let caps = reader.capabilities();
        assert!(caps.supports_schema_inference);
        assert!(caps.is_streaming);
        assert!(!caps.supports_partitioning);
    }
}
