//! CSV File Connector

use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;

use crate::catalog::{
    api_types::{
        ColumnDefinition, ConnectionTestResult, QueryResult, SchemaDefinition, TableDefinition,
    },
    connector::{
        ConnectorCapabilities, ConnectorResult, Credentials, DataSourceConnector, ValidationResult,
    },
    types::{CsvFileConfig, DataSource, SourceConfig},
};
use crate::errors::GraphicaError;

pub struct CsvConnector;

impl CsvConnector {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CsvConnector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DataSourceConnector for CsvConnector {
    fn name(&self) -> &'static str {
        "CSV File Connector"
    }

    fn source_type(&self) -> &'static str {
        "CsvFile"
    }

    fn validate_config(&self, config: &SourceConfig) -> ConnectorResult<ValidationResult> {
        match config {
            SourceConfig::CsvFile(csv_config) => {
                let mut errors = vec![];

                // Validate path
                if csv_config.path.is_empty() {
                    errors.push("Path cannot be empty".to_string());
                }

                // Validate delimiter (should be printable ASCII, common delimiters)
                let delimiter = csv_config.delimiter;
                if !delimiter.is_ascii() {
                    errors.push(format!(
                        "Delimiter must be ASCII character, got: '{}'",
                        delimiter
                    ));
                } else if delimiter == '\0' {
                    errors.push("Delimiter cannot be null character".to_string());
                } else if delimiter == '\n' || delimiter == '\r' {
                    errors.push("Delimiter cannot be newline character".to_string());
                } else if delimiter == '"' {
                    errors.push(
                        "Delimiter cannot be quote character (reserved for field quoting)"
                            .to_string(),
                    );
                }
                // Common valid delimiters: comma, tab, pipe, semicolon, space, etc.
                // We're lenient here - any other printable ASCII is allowed

                if errors.is_empty() {
                    Ok(ValidationResult::valid())
                } else {
                    Ok(ValidationResult::invalid(errors))
                }
            }
            _ => Err(GraphicaError::Configuration(
                "Expected CSV configuration".to_string(),
            )),
        }
    }

    async fn test_connection(
        &self,
        source: &DataSource,
        credentials: Credentials,
    ) -> ConnectorResult<ConnectionTestResult> {
        let config = match &source.connection.config {
            SourceConfig::CsvFile(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected CSV configuration".to_string(),
                ))
            }
        };

        let start = std::time::Instant::now();
        tracing::info!("CSV connection test: {}", config.path);

        let is_url = config.path.starts_with("http://") || config.path.starts_with("https://");

        if is_url {
            let client = Client::new();
            let mut request = client.head(&config.path);

            if let Some(token) = credentials
                .additional
                .get("token")
                .or_else(|| credentials.additional.get("access_token"))
            {
                request = request.bearer_auth(token);
            } else if !credentials.username.is_empty() || !credentials.password.is_empty() {
                request = request.basic_auth(
                    credentials.username.clone(),
                    Some(credentials.password.clone()),
                );
            }

            let response = request.send().await;
            match response {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let error_msg = format!("CSV URL returned status {}", resp.status());
                        return Ok(ConnectionTestResult {
                            success: false,
                            duration_ms: start.elapsed().as_millis() as u64,
                            error: Some(error_msg),
                            metadata: HashMap::from([("path".to_string(), config.path.clone())]),
                            tested_at: Utc::now(),
                        });
                    }
                }
                Err(e) => {
                    return Ok(ConnectionTestResult {
                        success: false,
                        duration_ms: start.elapsed().as_millis() as u64,
                        error: Some(format!("CSV URL request failed: {}", e)),
                        metadata: HashMap::from([("path".to_string(), config.path.clone())]),
                        tested_at: Utc::now(),
                    });
                }
            }

            let duration_ms = start.elapsed().as_millis() as u64;
            return Ok(ConnectionTestResult {
                success: true,
                duration_ms,
                error: None,
                metadata: HashMap::from([
                    ("path".to_string(), config.path.clone()),
                    ("delimiter".to_string(), config.delimiter.to_string()),
                    ("has_header".to_string(), config.has_header.to_string()),
                ]),
                tested_at: Utc::now(),
            });
        }

        // Check if file exists at path (local filesystem)
        let path = std::path::Path::new(&config.path);

        // Check file existence
        let metadata = match tokio::fs::metadata(path).await {
            Ok(m) => m,
            Err(e) => {
                let error_msg = format!("Failed to access CSV file at '{}': {}", config.path, e);
                tracing::error!("{}", error_msg);
                return Ok(ConnectionTestResult {
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    error: Some(error_msg),
                    metadata: HashMap::from([("path".to_string(), config.path.clone())]),
                    tested_at: Utc::now(),
                });
            }
        };

        // Verify it's a file, not a directory
        if !metadata.is_file() {
            let error_msg = format!("Path '{}' is not a file", config.path);
            tracing::error!("{}", error_msg);
            return Ok(ConnectionTestResult {
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
                error: Some(error_msg),
                metadata: HashMap::from([("path".to_string(), config.path.clone())]),
                tested_at: Utc::now(),
            });
        }

        // Check file size
        let file_size = metadata.len();
        if file_size == 0 {
            tracing::warn!("CSV file at '{}' is empty", config.path);
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        tracing::info!(
            "CSV connection test successful: {} ({} bytes, {} ms)",
            config.path,
            file_size,
            duration_ms
        );

        Ok(ConnectionTestResult {
            success: true,
            duration_ms,
            error: None,
            metadata: HashMap::from([
                ("path".to_string(), config.path.clone()),
                ("delimiter".to_string(), config.delimiter.to_string()),
                ("has_header".to_string(), config.has_header.to_string()),
                ("file_size_bytes".to_string(), file_size.to_string()),
            ]),
            tested_at: Utc::now(),
        })
    }

    async fn infer_schema(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        _table_name: Option<&str>,
        sample_size: usize,
    ) -> ConnectorResult<SchemaDefinition> {
        let config = match &source.connection.config {
            SourceConfig::CsvFile(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected CSV configuration".to_string(),
                ))
            }
        };

        tracing::info!(
            "CSV schema inference: {} (sample_size: {})",
            config.path,
            sample_size
        );

        let path = std::path::Path::new(&config.path);

        // Open CSV file with configured delimiter
        let file = tokio::fs::File::open(path).await?;

        // Convert to std::fs::File for csv crate (which expects sync IO)
        let std_file = file.into_std().await;

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(config.delimiter as u8)
            .has_headers(config.has_header)
            .from_reader(std_file);

        // Get column names and initialize samples
        let (headers, mut type_samples, mut row_count) = if config.has_header {
            // CSV has headers - read them first
            let hdrs = reader
                .headers()
                .map_err(|e| GraphicaError::Internal(format!("Failed to read CSV headers: {}", e)))?
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();

            let samples = vec![vec![]; hdrs.len()];
            (hdrs, samples, 0)
        } else {
            // No headers - peek at first record to determine column count
            let first_record = reader.records().next();
            match first_record {
                Some(Ok(record)) => {
                    let col_count = record.len();
                    let hdrs: Vec<String> = (0..col_count).map(|i| format!("col_{}", i)).collect();

                    // Initialize samples and add the first record we just consumed
                    let mut samples: Vec<Vec<String>> = vec![vec![]; col_count];
                    for (i, field) in record.iter().enumerate() {
                        if i < col_count {
                            samples[i].push(field.to_string());
                        }
                    }

                    (hdrs, samples, 1)
                }
                Some(Err(e)) => {
                    return Err(GraphicaError::Internal(format!(
                        "Failed to read first CSV record: {}",
                        e
                    )));
                }
                None => {
                    return Err(GraphicaError::Configuration(
                        "CSV file is empty".to_string(),
                    ));
                }
            }
        };

        if headers.is_empty() {
            return Err(GraphicaError::Configuration(
                "CSV file has no columns".to_string(),
            ));
        }

        // Sample remaining rows to infer types (up to sample_size total)
        for result in reader.records().take(sample_size.saturating_sub(row_count)) {
            let record = result.map_err(|e| {
                GraphicaError::Internal(format!("Failed to read CSV record: {}", e))
            })?;

            for (i, field) in record.iter().enumerate() {
                if i < type_samples.len() {
                    type_samples[i].push(field.to_string());
                }
            }
            row_count += 1;
        }

        // Infer data type for each column
        let columns = headers
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let samples = &type_samples[i];
                let data_type = infer_column_type(samples);

                ColumnDefinition {
                    name: name.clone(),
                    data_type,
                    nullable: true, // CSV columns are always nullable
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }
            })
            .collect();

        tracing::info!(
            "Inferred schema for CSV '{}': {} columns, {} sample rows",
            config.path,
            headers.len(),
            row_count
        );

        Ok(SchemaDefinition {
            name: path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("csv_file")
                .to_string(),
            tables: vec![TableDefinition {
                name: "data".to_string(),
                columns,
                estimated_rows: Some(row_count as u64),
            }],
            relationships: vec![],
            indexes: vec![],
            inferred_at: Utc::now(),
        })
    }

    async fn execute_query(
        &self,
        source: &DataSource,
        _credentials: Credentials,
        _query: &str,
        _parameters: HashMap<String, serde_json::Value>,
        limit: Option<usize>,
        _timeout_secs: u64,
    ) -> ConnectorResult<QueryResult> {
        let start = std::time::Instant::now();

        let config = match &source.connection.config {
            SourceConfig::CsvFile(c) => c,
            _ => {
                return Err(GraphicaError::Configuration(
                    "Expected CSV configuration".to_string(),
                ))
            }
        };

        tracing::info!("CSV query execution: {} (limit: {:?})", config.path, limit);

        let path = std::path::Path::new(&config.path);

        // Note: CSV doesn't support SQL queries natively
        // For now, we just read all rows (or up to limit) and return them as JSON
        // Future: integrate with DuckDB or DataFusion for SQL support

        // Open CSV file
        let file = tokio::fs::File::open(path).await?;
        let std_file = file.into_std().await;

        let mut reader = csv::ReaderBuilder::new()
            .delimiter(config.delimiter as u8)
            .has_headers(config.has_header)
            .from_reader(std_file);

        // Safety: Cap maximum rows to prevent OOM (unless explicitly set lower by caller)
        const MAX_SAFE_ROWS: usize = 1_000_000;
        let max_rows = limit.unwrap_or(MAX_SAFE_ROWS).min(MAX_SAFE_ROWS);
        let explicitly_limited = limit.is_some();

        // Get column names and potentially first row
        let (headers, mut rows, initial_count) = if config.has_header {
            // CSV has headers - read them first
            let hdrs = reader
                .headers()
                .map_err(|e| GraphicaError::Internal(format!("Failed to read CSV headers: {}", e)))?
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();

            (hdrs, Vec::new(), 0)
        } else {
            // No headers - consume first record to determine column count
            let first_record = reader.records().next();
            match first_record {
                Some(Ok(record)) => {
                    let col_count = record.len();
                    let hdrs: Vec<String> = (0..col_count).map(|i| format!("col_{}", i)).collect();

                    // Convert first record to JSON
                    let mut row: HashMap<String, serde_json::Value> = HashMap::new();
                    for (i, field) in record.iter().enumerate() {
                        if i < hdrs.len() {
                            row.insert(
                                hdrs[i].clone(),
                                serde_json::Value::String(field.to_string()),
                            );
                        }
                    }

                    let mut rows_vec = Vec::new();
                    rows_vec.push(serde_json::to_value(row).unwrap());

                    (hdrs, rows_vec, 1)
                }
                Some(Err(e)) => {
                    return Err(GraphicaError::Internal(format!(
                        "Failed to read first CSV record: {}",
                        e
                    )));
                }
                None => {
                    return Err(GraphicaError::Configuration(
                        "CSV file is empty".to_string(),
                    ));
                }
            }
        };

        if headers.is_empty() {
            return Err(GraphicaError::Configuration(
                "CSV file has no columns".to_string(),
            ));
        }

        // Read remaining rows (up to max_rows total)
        let mut row_number = initial_count + 1; // For error reporting
        let mut truncated = false;

        for result in reader
            .records()
            .take(max_rows.saturating_sub(initial_count))
        {
            let record = result.map_err(|e| {
                GraphicaError::Internal(format!(
                    "Failed to read CSV record at row {}: {}",
                    row_number, e
                ))
            })?;

            // Convert record to JSON object
            let mut row: HashMap<String, serde_json::Value> = HashMap::new();
            for (i, field) in record.iter().enumerate() {
                if i < headers.len() {
                    row.insert(
                        headers[i].clone(),
                        serde_json::Value::String(field.to_string()),
                    );
                }
            }
            rows.push(serde_json::to_value(row).unwrap());
            row_number += 1;
        }

        // Check if there are more rows (for truncation flag)
        if rows.len() >= max_rows {
            // Check if there's at least one more record
            if reader.records().next().is_some() {
                truncated = true;
            }
        }

        // Create column definitions
        let column_defs: Vec<ColumnDefinition> = headers
            .iter()
            .map(|name| {
                ColumnDefinition {
                    name: name.clone(),
                    data_type: "string".to_string(), // CSV data is always string initially
                    nullable: true,
                    primary_key: false,
                    default_value: None,
                    semantic_type: None,
                    statistics: None,
                }
            })
            .collect();

        let execution_time_ms = start.elapsed().as_millis() as u64;
        let row_count = rows.len();

        tracing::info!(
            "CSV query executed: {} rows returned in {} ms (truncated: {})",
            row_count,
            execution_time_ms,
            truncated
        );

        Ok(QueryResult {
            rows,
            row_count,
            execution_time_ms,
            truncated,
            columns: Some(column_defs),
        })
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            parameterized_queries: false,
            schema_inference: true,
            query_timeout: false,
            streaming: true,
            transactions: false,
            max_batch_size: Some(100000),
        }
    }
}

/// Infer column data type from sample values
fn infer_column_type(samples: &[String]) -> String {
    if samples.is_empty() {
        return "string".to_string();
    }

    // Count how many samples match each type
    let mut int_count = 0;
    let mut float_count = 0;
    let mut bool_count = 0;
    let mut date_count = 0;
    let mut timestamp_count = 0;

    for sample in samples {
        let trimmed = sample.trim();

        // Skip empty values
        if trimmed.is_empty() {
            continue;
        }

        // Check boolean FIRST (before integer) to avoid "1"/"0" ambiguity
        // Only match explicit boolean strings, not numeric 1/0
        let lower = trimmed.to_lowercase();
        if matches!(
            lower.as_str(),
            "true" | "false" | "t" | "f" | "yes" | "no" | "y" | "n"
        ) {
            bool_count += 1;
            continue;
        }

        // Check timestamp (ISO 8601 with time: YYYY-MM-DDTHH:MM:SS or similar)
        if trimmed.len() >= 19 {
            // Try parsing as RFC3339/ISO8601 timestamp
            if chrono::DateTime::parse_from_rfc3339(trimmed).is_ok()
                || chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S").is_ok()
                || chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S").is_ok()
            {
                timestamp_count += 1;
                continue;
            }
        }

        // Check date (basic ISO 8601 format: YYYY-MM-DD)
        if trimmed.len() == 10
            && trimmed.chars().nth(4) == Some('-')
            && trimmed.chars().nth(7) == Some('-')
        {
            if chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_ok() {
                date_count += 1;
                continue;
            }
        }

        // Check integer
        if trimmed.parse::<i64>().is_ok() {
            int_count += 1;
            continue;
        }

        // Check float (this will also match integers, so check after integer)
        if trimmed.parse::<f64>().is_ok() {
            float_count += 1;
            continue;
        }
    }

    let non_empty_count = samples.iter().filter(|s| !s.trim().is_empty()).count();

    // Type decision based on majority (>80% threshold for typed data)
    if non_empty_count > 0 {
        let threshold = (non_empty_count as f64 * 0.8) as usize;

        // Check timestamp before date (more specific)
        if timestamp_count >= threshold {
            return "timestamp".to_string();
        }

        if date_count >= threshold {
            return "date".to_string();
        }

        if bool_count >= threshold {
            return "boolean".to_string();
        }

        if int_count >= threshold {
            return "integer".to_string();
        }

        if (int_count + float_count) >= threshold {
            return "double".to_string();
        }
    }

    // Default to string
    "string".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::types::ConnectionDetails;
    use std::io::Write;

    fn create_test_source() -> DataSource {
        DataSource::new(
            "Test CSV".to_string(),
            "CsvFile".to_string(),
            ConnectionDetails {
                secret_ref: "none".to_string(),
                config: SourceConfig::CsvFile(CsvFileConfig {
                    path: "/data/sample.csv".to_string(),
                    delimiter: ',',
                    has_header: true,
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
        )
    }

    #[test]
    fn test_validate_config() {
        let connector = CsvConnector::new();
        let source = create_test_source();

        let result = connector
            .validate_config(&source.connection.config)
            .unwrap();
        assert!(result.valid);
    }

    #[tokio::test]
    async fn test_connection() {
        let mut temp_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(temp_file, "id,name").unwrap();
        writeln!(temp_file, "1,Alice").unwrap();

        let connector = CsvConnector::new();
        let source = DataSource::new(
            "Test CSV".to_string(),
            "CsvFile".to_string(),
            ConnectionDetails {
                secret_ref: "none".to_string(),
                config: SourceConfig::CsvFile(CsvFileConfig {
                    path: temp_file.path().to_string_lossy().to_string(),
                    delimiter: ',',
                    has_header: true,
                }),
                encryption_enabled: false,
                credentials: Default::default(),
            },
        );
        let creds = Credentials::new("".to_string(), "".to_string());

        let result = connector.test_connection(&source, creds).await.unwrap();
        assert!(result.success);
    }
}
