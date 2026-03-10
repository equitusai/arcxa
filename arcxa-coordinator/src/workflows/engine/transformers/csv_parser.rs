//! CSV Parser Transformer
//!
//! Reads CSV files from the file library and converts them to JSON row format.
//!
//! ## Configuration
//!
//! ```json
//! {
//!   "file_id": "file_abc123",           // Required: File library file ID
//!   "delimiter": ",",                    // Optional: Override file metadata (default: from file)
//!   "has_header": true,                  // Optional: Override file metadata (default: from file)
//!   "encoding": "utf-8",                 // Optional: Override file metadata (default: from file)
//!   "skip_rows": 0,                      // Optional: Number of rows to skip (default: 0)
//!   "max_rows": null,                    // Optional: Maximum rows to parse (default: all)
//!   "trim_whitespace": true,             // Optional: Trim field whitespace (default: true)
//!   "skip_empty_rows": true              // Optional: Skip empty rows (default: true)
//! }
//! ```
//!
//! ## Output Format
//!
//! Adds parsed CSV rows to the data object:
//!
//! ```json
//! {
//!   "file_id": "file_abc123",
//!   "file_name": "customers.csv",
//!   "row_count": 1000,
//!   "column_names": ["customer_id", "first_name", "last_name", "email"],
//!   "rows": [
//!     {"customer_id": "1", "first_name": "John", "last_name": "Doe", "email": "john@example.com"},
//!     {"customer_id": "2", "first_name": "Jane", "last_name": "Smith", "email": "jane@example.com"}
//!   ]
//! }
//! ```

use super::Transformer;
use crate::api::file_library::storage_trait::FileLibraryStore;
use crate::etl::readers::csv::{CsvOptions, CsvReader};
use crate::etl::traits::FormatReader;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// CSV parser transformer
///
/// Reads CSV files from file library storage and converts them to JSON rows.
pub struct CsvParserTransformer {
    file_store: Arc<dyn FileLibraryStore>,
}

impl CsvParserTransformer {
    /// Create a new CSV parser transformer
    ///
    /// # Arguments
    ///
    /// * `file_store` - File library storage for reading files
    pub fn new(file_store: Arc<dyn FileLibraryStore>) -> Self {
        Self { file_store }
    }

    /// Parse configuration and create CsvReader
    fn create_reader(&self, config: &JsonValue) -> Result<(CsvReader, CsvParseConfig)> {
        let parse_config = CsvParseConfig::from_json(config)?;

        // Get file metadata to merge with config overrides
        let file_metadata = self
            .file_store
            .get_file(&parse_config.file_id)
            .context("Failed to get file from file library")?
            .ok_or_else(|| anyhow!("File not found: {}", parse_config.file_id))?;

        // Build CsvOptions from config (with file metadata as defaults)
        let delimiter = parse_config
            .delimiter
            .unwrap_or_else(|| file_metadata.delimiter.chars().next().unwrap_or(','));

        let has_header = parse_config.has_header.unwrap_or(file_metadata.has_header);

        let options = CsvOptions {
            delimiter: delimiter as u8,
            has_header,
            quote_char: b'"',
            escape_char: None,
            skip_rows: parse_config.skip_rows,
            expected_columns: None,
            trim_whitespace: parse_config.trim_whitespace,
        };

        let reader = CsvReader::new(
            self.file_store.clone(),
            parse_config.file_id.clone(),
            options,
        );

        Ok((reader, parse_config))
    }
}

#[async_trait]
impl Transformer for CsvParserTransformer {
    async fn transform(
        &self,
        config: &JsonValue,
        data: &mut JsonValue,
        _context: Option<&crate::workflows::engine::executor::ExecutionContext>,
    ) -> Result<()> {
        // Create CsvReader with configuration
        let (csv_reader, parse_config) = self.create_reader(config)?;

        // Get file metadata for output
        let file_metadata = self
            .file_store
            .get_file(&parse_config.file_id)
            .context("Failed to get file from file library")?
            .ok_or_else(|| anyhow!("File not found: {}", parse_config.file_id))?;

        info!(
            "Parsing CSV file: {} ({})",
            file_metadata.name, parse_config.file_id
        );

        // Get schema for column names
        let schema = csv_reader
            .infer_schema()
            .await
            .context("Failed to infer CSV schema")?;

        // Convert column names to match old transformer format (column_N -> col_N for backward compat)
        let column_names: Vec<String> = schema
            .fields
            .iter()
            .map(|f| {
                if f.name.starts_with("column_") {
                    f.name.replace("column_", "col_")
                } else {
                    f.name.clone()
                }
            })
            .collect();

        debug!("CSV columns: {:?}", column_names);

        // Read all records using CsvReader stream
        let mut stream = csv_reader
            .read_stream()
            .await
            .context("Failed to create CSV stream")?;

        let mut rows: Vec<JsonValue> = Vec::new();
        let mut row_count = 0;
        let mut skipped_count = 0;
        let mut error_count = 0;
        let mut line_number = 0;

        while let Some(record_result) = stream.next().await {
            line_number += 1;

            match record_result {
                Ok(record) => {
                    // Skip empty rows if configured
                    if parse_config.skip_empty_rows {
                        let is_empty = record
                            .data
                            .as_object()
                            .map(|obj| {
                                obj.values()
                                    .all(|v| v.as_str().map_or(true, |s| s.trim().is_empty()))
                            })
                            .unwrap_or(true);

                        if is_empty {
                            skipped_count += 1;
                            continue;
                        }
                    }

                    // Stop if max_rows reached
                    if let Some(max_rows) = parse_config.max_rows {
                        if row_count >= max_rows {
                            debug!("Reached max_rows limit: {}", max_rows);
                            break;
                        }
                    }

                    // Rename column_N to col_N in record data for backward compatibility
                    let row_data = if let Some(obj) = record.data.as_object() {
                        let mut renamed = serde_json::Map::new();
                        for (key, value) in obj {
                            let new_key = if key.starts_with("column_") {
                                key.replace("column_", "col_")
                            } else {
                                key.clone()
                            };
                            renamed.insert(new_key, value.clone());
                        }
                        JsonValue::Object(renamed)
                    } else {
                        record.data
                    };

                    rows.push(row_data);
                    row_count += 1;
                }
                Err(e) => {
                    error_count += 1;
                    warn!("Failed to parse row {}: {}", line_number, e);
                    // Continue parsing - don't fail entire transformation for one bad row
                }
            }
        }

        // Get stats for delimiter info
        let stats = csv_reader
            .get_stats()
            .await
            .context("Failed to get CSV stats")?;

        let delimiter = stats
            .metadata
            .get("delimiter")
            .and_then(|v| v.as_str())
            .unwrap_or(",")
            .to_string();

        let has_header = stats
            .metadata
            .get("has_header")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        info!(
            "CSV parsing complete: {} rows, {} skipped, {} errors",
            row_count, skipped_count, error_count
        );

        // Update data object with parsed results (maintaining backward compatible format)
        data["file_id"] = json!(parse_config.file_id);
        data["file_name"] = json!(file_metadata.name);
        data["row_count"] = json!(row_count);
        data["column_names"] = json!(column_names);
        data["rows"] = json!(rows);

        // Include parsing statistics (same format as before)
        data["parsing_stats"] = json!({
            "total_rows_parsed": row_count,
            "rows_skipped": skipped_count,
            "rows_with_errors": error_count,
            "has_header": has_header,
            "delimiter": delimiter,
        });

        Ok(())
    }

    fn name(&self) -> &'static str {
        "csv_parser"
    }

    fn validate_config(&self, config: &JsonValue) -> Result<()> {
        // Validate that file_id is present
        if !config.get("file_id").and_then(|v| v.as_str()).is_some() {
            anyhow::bail!("Missing required field: file_id");
        }

        // Validate delimiter if provided
        if let Some(delimiter) = config.get("delimiter").and_then(|v| v.as_str()) {
            if delimiter.is_empty() {
                anyhow::bail!("Delimiter cannot be empty");
            }
            if delimiter.len() > 1 {
                anyhow::bail!("Delimiter must be a single character");
            }
        }

        // Validate skip_rows if provided
        if let Some(skip_rows) = config.get("skip_rows").and_then(|v| v.as_i64()) {
            if skip_rows < 0 {
                anyhow::bail!("skip_rows cannot be negative");
            }
        }

        // Validate max_rows if provided
        if let Some(max_rows) = config.get("max_rows").and_then(|v| v.as_i64()) {
            if max_rows <= 0 {
                anyhow::bail!("max_rows must be positive");
            }
        }

        Ok(())
    }
}

/// CSV parsing configuration
#[derive(Debug, Clone)]
struct CsvParseConfig {
    file_id: String,
    delimiter: Option<char>,
    has_header: Option<bool>,
    encoding: Option<String>,
    skip_rows: usize,
    max_rows: Option<usize>,
    trim_whitespace: bool,
    skip_empty_rows: bool,
}

impl CsvParseConfig {
    /// Parse configuration from JSON
    fn from_json(config: &JsonValue) -> Result<Self> {
        let file_id = config
            .get("file_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required field: file_id"))?
            .to_string();

        Ok(CsvParseConfig {
            file_id,
            delimiter: config
                .get("delimiter")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().next().unwrap_or(',')),
            has_header: config.get("has_header").and_then(|v| v.as_bool()),
            encoding: config
                .get("encoding")
                .and_then(|v| v.as_str())
                .map(String::from),
            skip_rows: config
                .get("skip_rows")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize,
            max_rows: config
                .get("max_rows")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize),
            trim_whitespace: config
                .get("trim_whitespace")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            skip_empty_rows: config
                .get("skip_empty_rows")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::file_library::types::{DataFile, FileOwner, FileStatus};
    use chrono::Utc;
    use std::collections::HashMap;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Mock file store for testing
    struct MockFileStore {
        files: HashMap<String, DataFile>,
    }

    impl MockFileStore {
        fn new() -> Self {
            Self {
                files: HashMap::new(),
            }
        }

        fn add_file(&mut self, file: DataFile) {
            self.files.insert(file.id.clone(), file);
        }
    }

    impl FileLibraryStore for MockFileStore {
        fn create_file(&self, _file: DataFile) -> Result<()> {
            unimplemented!()
        }

        fn get_file(&self, file_id: &str) -> Result<Option<DataFile>> {
            Ok(self.files.get(file_id).cloned())
        }

        fn update_file(
            &self,
            _file_id: &str,
            _updates: crate::api::file_library::types::UpdateFileRequest,
        ) -> Result<DataFile> {
            unimplemented!()
        }

        fn delete_file(&self, _file_id: &str) -> Result<()> {
            unimplemented!()
        }

        fn update_last_accessed(&self, _file_id: &str) -> Result<()> {
            Ok(())
        }

        fn list_files(
            &self,
            _request: &crate::api::file_library::types::ListFilesRequest,
        ) -> Result<Vec<DataFile>> {
            unimplemented!()
        }

        fn search_files(
            &self,
            _request: &crate::api::file_library::types::SearchRequest,
        ) -> Result<Vec<DataFile>> {
            unimplemented!()
        }

        fn create_folder(
            &self,
            _folder: crate::api::file_library::types::Folder,
        ) -> Result<crate::api::file_library::types::Folder> {
            unimplemented!()
        }

        fn get_folder(
            &self,
            _folder_id: &str,
        ) -> Result<Option<crate::api::file_library::types::Folder>> {
            unimplemented!()
        }

        fn list_folders(&self) -> Result<Vec<crate::api::file_library::types::Folder>> {
            unimplemented!()
        }

        fn update_folder(
            &self,
            _folder_id: &str,
            _updates: crate::api::file_library::types::UpdateFolderRequest,
        ) -> Result<crate::api::file_library::types::Folder> {
            unimplemented!()
        }

        fn delete_folder(&self, _folder_id: &str, _force: bool) -> Result<()> {
            unimplemented!()
        }

        fn create_job(&self, _job: crate::api::file_library::types::ImportJob) -> Result<()> {
            unimplemented!()
        }

        fn get_job(
            &self,
            _job_id: &str,
        ) -> Result<Option<crate::api::file_library::types::ImportJob>> {
            unimplemented!()
        }

        fn update_job(&self, _job: crate::api::file_library::types::ImportJob) -> Result<()> {
            unimplemented!()
        }

        fn update_job_progress(
            &self,
            _job_id: &str,
            _processed_files: usize,
            _progress_percent: f32,
        ) -> Result<()> {
            unimplemented!()
        }

        fn complete_job(
            &self,
            _job_id: &str,
            _status: crate::api::file_library::types::JobStatus,
            _successful_files: usize,
            _failed_files: usize,
            _results: Vec<crate::api::file_library::types::ImportResult>,
            _duration_ms: u64,
        ) -> Result<()> {
            unimplemented!()
        }

        fn list_tags(&self) -> Result<Vec<crate::api::file_library::types::TagInfo>> {
            unimplemented!()
        }

        fn get_statistics(&self) -> Result<crate::api::file_library::types::LibraryStatsResponse> {
            unimplemented!()
        }
    }

    fn create_test_csv_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    fn create_test_file_metadata(file_path: String) -> DataFile {
        DataFile {
            id: "file_test_123".to_string(),
            name: "test.csv".to_string(),
            file_path,
            folder_id: None,
            description: None,
            owner: FileOwner {
                user_id: "user_1".to_string(),
                email: "test@example.com".to_string(),
                name: "Test User".to_string(),
            },
            size_bytes: 1024,
            encoding: "utf-8".to_string(),
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
        }
    }

    #[tokio::test]
    async fn test_csv_parsing_with_header() {
        let csv_content = "id,name,email\n1,Alice,alice@example.com\n2,Bob,bob@example.com\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123"
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["file_id"], "file_test_123");
        assert_eq!(data["file_name"], "test.csv");
        assert_eq!(data["row_count"], 2);
        assert_eq!(data["column_names"], json!(["id", "name", "email"]));

        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["id"], "1");
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[1]["email"], "bob@example.com");
    }

    #[tokio::test]
    async fn test_csv_parsing_without_header() {
        let csv_content = "1,Alice,alice@example.com\n2,Bob,bob@example.com\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.has_header = false;
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "has_header": false
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        assert_eq!(data["column_names"], json!(["col_0", "col_1", "col_2"]));

        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["col_0"], "1");
        assert_eq!(rows[0]["col_1"], "Alice");
    }

    #[tokio::test]
    async fn test_csv_parsing_with_skip_rows() {
        let csv_content = "id,name,email\n1,Alice,alice@example.com\n2,Bob,bob@example.com\n3,Carol,carol@example.com\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "skip_rows": 1
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["id"], "2"); // First row (Alice) was skipped
    }

    #[tokio::test]
    async fn test_csv_parsing_with_max_rows() {
        let csv_content = "id,name,email\n1,Alice,alice@example.com\n2,Bob,bob@example.com\n3,Carol,carol@example.com\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "max_rows": 2
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn test_validation_missing_file_id() {
        let mock_store = MockFileStore::new();
        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({});
        let result = transformer.validate_config(&config);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("file_id"));
    }

    #[tokio::test]
    async fn test_validation_invalid_delimiter() {
        let mock_store = MockFileStore::new();
        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_123",
            "delimiter": "abc"
        });
        let result = transformer.validate_config(&config);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("single character"));
    }

    #[tokio::test]
    async fn test_file_not_found() {
        let mock_store = MockFileStore::new();
        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "nonexistent_file"
        });

        let mut data = json!({});
        let result = transformer.transform(&config, &mut data, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("File not found"));
    }

    // ============================================================================
    // Backward Compatibility Tests (Migration to CsvReader)
    // ============================================================================

    #[tokio::test]
    async fn test_backward_compat_delimiter_pipe() {
        let csv_content = "name|age|city\nAlice|30|NYC\nBob|25|LA\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.delimiter = "|".to_string();
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "delimiter": "|"
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        assert_eq!(data["column_names"], json!(["name", "age", "city"]));

        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "Alice");
        assert_eq!(rows[0]["age"], "30");
        assert_eq!(rows[1]["city"], "LA");

        // Verify parsing_stats exists and has correct format
        assert!(data["parsing_stats"].is_object());
        assert_eq!(data["parsing_stats"]["delimiter"], "|");
    }

    #[tokio::test]
    async fn test_backward_compat_delimiter_tab() {
        let csv_content = "name\tage\tcity\nAlice\t30\tNYC\nBob\t25\tLA\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.delimiter = "\t".to_string();
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "delimiter": "\t"
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["city"], "NYC");
    }

    #[tokio::test]
    async fn test_backward_compat_delimiter_semicolon() {
        let csv_content = "name;age;city\nAlice;30;NYC\nBob;25;LA\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.delimiter = ";".to_string();
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "delimiter": ";"
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        assert_eq!(data["parsing_stats"]["delimiter"], ";");
    }

    #[tokio::test]
    async fn test_backward_compat_output_format() {
        // Test that the output format exactly matches the old transformer
        let csv_content = "id,name\n1,Alice\n2,Bob\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123"
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        // Verify all expected fields exist
        assert!(data.get("file_id").is_some());
        assert!(data.get("file_name").is_some());
        assert!(data.get("row_count").is_some());
        assert!(data.get("column_names").is_some());
        assert!(data.get("rows").is_some());
        assert!(data.get("parsing_stats").is_some());

        // Verify parsing_stats structure
        let stats = &data["parsing_stats"];
        assert!(stats.get("total_rows_parsed").is_some());
        assert!(stats.get("rows_skipped").is_some());
        assert!(stats.get("rows_with_errors").is_some());
        assert!(stats.get("has_header").is_some());
        assert!(stats.get("delimiter").is_some());

        assert_eq!(stats["total_rows_parsed"], 2);
        assert_eq!(stats["rows_skipped"], 0);
        assert_eq!(stats["rows_with_errors"], 0);
        assert_eq!(stats["has_header"], true);
    }

    #[tokio::test]
    async fn test_backward_compat_skip_empty_rows() {
        let csv_content = "id,name\n1,Alice\n\n2,Bob\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "skip_empty_rows": true
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        // Empty row should be skipped (note: completely empty lines are skipped by the CSV reader itself,
        // so they don't show up in rows_skipped count. The skip_empty_rows feature skips rows that have
        // fields but they're all empty/whitespace)
        assert_eq!(data["row_count"], 2);
        assert_eq!(data["parsing_stats"]["rows_skipped"], 0);
    }

    #[tokio::test]
    async fn test_backward_compat_trim_whitespace() {
        let csv_content = "id,name\n  1  ,  Alice  \n  2  ,  Bob  \n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "trim_whitespace": true
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        let rows = data["rows"].as_array().unwrap();
        // With trimming enabled, spaces should be removed
        assert_eq!(rows[0]["id"], "1");
        assert_eq!(rows[0]["name"], "Alice");
    }

    #[tokio::test]
    async fn test_backward_compat_config_override_delimiter() {
        // Test that config delimiter overrides file metadata
        let csv_content = "name|age\nAlice|30\nBob|25\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.delimiter = ",".to_string(); // File says comma
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "delimiter": "|" // But config overrides with pipe
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        assert_eq!(data["row_count"], 2);
        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["name"], "Alice");
    }

    #[tokio::test]
    async fn test_backward_compat_config_override_has_header() {
        let csv_content = "Alice,30\nBob,25\n";
        let temp_file = create_test_csv_file(csv_content);

        let mut mock_store = MockFileStore::new();
        let mut file_metadata =
            create_test_file_metadata(temp_file.path().to_str().unwrap().to_string());
        file_metadata.has_header = true; // File says it has headers
        mock_store.add_file(file_metadata);

        let transformer = CsvParserTransformer::new(Arc::new(mock_store));

        let config = json!({
            "file_id": "file_test_123",
            "has_header": false // But config overrides to no headers
        });

        let mut data = json!({});
        transformer
            .transform(&config, &mut data, None)
            .await
            .unwrap();

        // Should generate col_0, col_1 (not Alice, 30)
        assert_eq!(data["column_names"], json!(["col_0", "col_1"]));
        let rows = data["rows"].as_array().unwrap();
        assert_eq!(rows[0]["col_0"], "Alice");
        assert_eq!(rows[0]["col_1"], "30");
    }
}
