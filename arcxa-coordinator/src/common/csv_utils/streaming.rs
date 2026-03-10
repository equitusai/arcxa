//! CSV Streaming Layer
//!
//! Production-grade streaming CSV reader with:
//! - Constant memory usage (no buffering entire file)
//! - Progress tracking
//! - Error recovery
//! - Performance metrics
//!
//! Used by the Data Loader for processing large CSV files.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::core::{detect_delimiter_advanced, detect_encoding_advanced, CsvDetectionConfig};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for CSV streaming reader
#[derive(Debug, Clone)]
pub struct CsvReaderConfig {
    /// Delimiter (auto-detect if None)
    pub delimiter: Option<u8>,

    /// Encoding (auto-detect if None)
    pub encoding: Option<String>,

    /// Has header row
    pub has_header: bool,

    /// Skip first N rows
    pub skip_rows: usize,

    /// Batch size for yielding records
    pub batch_size: usize,

    /// Skip rows with errors (vs fail fast)
    pub skip_errors: bool,

    /// Maximum field size (bytes)
    pub max_field_size: usize,

    /// Buffer capacity (bytes)
    pub buffer_capacity: usize,
}

impl Default for CsvReaderConfig {
    fn default() -> Self {
        Self {
            delimiter: None,
            encoding: None,
            has_header: true,
            skip_rows: 0,
            batch_size: 1000,
            skip_errors: false,
            max_field_size: 1024 * 1024, // 1MB
            buffer_capacity: 8192,
        }
    }
}

// ============================================================================
// Types
// ============================================================================

/// CSV reading error
#[derive(Debug, thiserror::Error)]
pub enum CsvError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CSV parsing error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("Encoding error: {0}")]
    Encoding(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Progress information
#[derive(Debug, Clone)]
pub struct ReaderProgress {
    pub bytes_read: u64,
    pub total_bytes: Option<u64>,
    pub rows_read: u64,
    pub rows_skipped: u64,
    pub errors_encountered: u64,
    pub elapsed_seconds: f64,
    pub rows_per_second: f64,
}

impl ReaderProgress {
    pub fn progress_percent(&self) -> Option<f64> {
        self.total_bytes.map(|total| {
            if total > 0 {
                (self.bytes_read as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        })
    }
}

/// CSV record (row)
#[derive(Debug, Clone)]
pub struct CsvRecord {
    pub row_number: u64,
    pub fields: Vec<String>,
}

// ============================================================================
// Streaming Reader
// ============================================================================

/// Streaming CSV reader with progress tracking
pub struct CsvStreamReader {
    config: CsvReaderConfig,
    file_path: PathBuf,
    reader: Option<csv::Reader<BufReader<File>>>,
    headers: Option<Vec<String>>,

    // Progress tracking
    rows_read: u64,
    rows_skipped: u64,
    errors_encountered: u64,
    bytes_read: u64,
    total_bytes: Option<u64>,
    start_time: Instant,
}

impl CsvStreamReader {
    /// Create new reader with auto-detection
    pub fn new(file_path: impl AsRef<Path>, mut config: CsvReaderConfig) -> Result<Self> {
        let file_path = file_path.as_ref().to_path_buf();

        // Auto-detect delimiter if not provided
        if config.delimiter.is_none() {
            let detection_config = CsvDetectionConfig::default();
            let delimiter_result = detect_delimiter_advanced(&file_path, &detection_config)
                .context("Failed to auto-detect delimiter")?;

            config.delimiter = Some(delimiter_result.delimiter.as_bytes()[0]);
        }

        // Auto-detect encoding if not provided
        if config.encoding.is_none() {
            let encoding =
                detect_encoding_advanced(&file_path).context("Failed to auto-detect encoding")?;
            config.encoding = Some(encoding.as_str().to_string());
        }

        // Get file size for progress tracking
        let total_bytes = std::fs::metadata(&file_path).ok().map(|meta| meta.len());

        Ok(Self {
            config,
            file_path,
            reader: None,
            headers: None,
            rows_read: 0,
            rows_skipped: 0,
            errors_encountered: 0,
            bytes_read: 0,
            total_bytes,
            start_time: Instant::now(),
        })
    }

    /// Create reader with explicit configuration (no auto-detection)
    pub fn with_config(file_path: impl AsRef<Path>, config: CsvReaderConfig) -> Result<Self> {
        let file_path = file_path.as_ref().to_path_buf();

        if config.delimiter.is_none() {
            return Err(anyhow!(
                "Delimiter must be specified when using with_config"
            ));
        }

        let total_bytes = std::fs::metadata(&file_path).ok().map(|meta| meta.len());

        Ok(Self {
            config,
            file_path,
            reader: None,
            headers: None,
            rows_read: 0,
            rows_skipped: 0,
            errors_encountered: 0,
            bytes_read: 0,
            total_bytes,
            start_time: Instant::now(),
        })
    }

    /// Initialize the reader (opens file)
    pub fn init(&mut self) -> Result<()> {
        let file = File::open(&self.file_path)
            .with_context(|| format!("Failed to open CSV file: {:?}", self.file_path))?;

        let buf_reader = BufReader::with_capacity(self.config.buffer_capacity, file);

        let mut csv_reader = csv::ReaderBuilder::new()
            .delimiter(self.config.delimiter.unwrap())
            .has_headers(self.config.has_header)
            .flexible(self.config.skip_errors)
            .from_reader(buf_reader);

        // Read headers if present
        if self.config.has_header {
            let headers = csv_reader
                .headers()
                .context("Failed to read CSV headers")?
                .iter()
                .map(|h| h.to_string())
                .collect();
            self.headers = Some(headers);
        }

        // Skip initial rows if configured
        for _ in 0..self.config.skip_rows {
            let mut record = csv::StringRecord::new();
            if csv_reader.read_record(&mut record)? {
                self.rows_skipped += 1;
            }
        }

        self.reader = Some(csv_reader);
        self.start_time = Instant::now();

        Ok(())
    }

    /// Get headers (if has_header = true)
    pub fn headers(&self) -> Option<&[String]> {
        self.headers.as_deref()
    }

    /// Read next record
    pub fn read_record(&mut self) -> Result<Option<CsvRecord>> {
        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| anyhow!("Reader not initialized. Call init() first."))?;

        let mut record = csv::StringRecord::new();

        match reader.read_record(&mut record) {
            Ok(true) => {
                self.rows_read += 1;
                self.bytes_read = reader.position().byte();

                let fields: Vec<String> = record.iter().map(|f| f.to_string()).collect();

                Ok(Some(CsvRecord {
                    row_number: self.rows_read,
                    fields,
                }))
            }
            Ok(false) => {
                // EOF
                Ok(None)
            }
            Err(e) => {
                if self.config.skip_errors {
                    self.errors_encountered += 1;
                    self.rows_skipped += 1;
                    // Skip this error and continue
                    self.read_record()
                } else {
                    Err(CsvError::Parse {
                        line: (self.rows_read + 1) as usize,
                        message: e.to_string(),
                    }
                    .into())
                }
            }
        }
    }

    /// Read records in batches
    pub fn read_batch(&mut self, batch_size: usize) -> Result<Vec<CsvRecord>> {
        let mut batch = Vec::with_capacity(batch_size);

        for _ in 0..batch_size {
            match self.read_record()? {
                Some(record) => batch.push(record),
                None => break,
            }
        }

        Ok(batch)
    }

    /// Get current progress
    pub fn progress(&self) -> ReaderProgress {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let rows_per_second = if elapsed > 0.0 {
            self.rows_read as f64 / elapsed
        } else {
            0.0
        };

        ReaderProgress {
            bytes_read: self.bytes_read,
            total_bytes: self.total_bytes,
            rows_read: self.rows_read,
            rows_skipped: self.rows_skipped,
            errors_encountered: self.errors_encountered,
            elapsed_seconds: elapsed,
            rows_per_second,
        }
    }

    /// Reset reader (re-open file)
    pub fn reset(&mut self) -> Result<()> {
        self.reader = None;
        self.headers = None;
        self.rows_read = 0;
        self.rows_skipped = 0;
        self.errors_encountered = 0;
        self.bytes_read = 0;
        self.start_time = Instant::now();

        self.init()
    }

    /// Iterate over all records
    pub fn iter_records(&mut self) -> CsvRecordIterator<'_> {
        CsvRecordIterator { reader: self }
    }
}

// ============================================================================
// Iterator
// ============================================================================

/// Iterator over CSV records
pub struct CsvRecordIterator<'a> {
    reader: &'a mut CsvStreamReader,
}

impl<'a> Iterator for CsvRecordIterator<'a> {
    type Item = Result<CsvRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.reader.read_record() {
            Ok(Some(record)) => Some(Ok(record)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Count total rows in CSV file (for progress estimation)
pub fn count_rows(file_path: impl AsRef<Path>, has_header: bool) -> Result<u64> {
    let file = File::open(file_path.as_ref())?;
    let reader = BufReader::new(file);

    let mut count = 0u64;
    for line in std::io::BufRead::lines(reader) {
        line?;
        count += 1;
    }

    // Subtract header if present
    if has_header && count > 0 {
        count -= 1;
    }

    Ok(count)
}

/// Estimate rows from file size (rough heuristic)
pub fn estimate_rows(file_path: impl AsRef<Path>, avg_row_size_bytes: u64) -> Result<u64> {
    let metadata = std::fs::metadata(file_path.as_ref())?;
    let file_size = metadata.len();

    if avg_row_size_bytes == 0 {
        return Ok(0);
    }

    Ok(file_size / avg_row_size_bytes)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_csv() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "name,age,city").unwrap();
        writeln!(file, "Alice,30,NYC").unwrap();
        writeln!(file, "Bob,25,LA").unwrap();
        writeln!(file, "Charlie,35,SF").unwrap();
        file
    }

    #[test]
    fn test_csv_stream_reader() {
        let temp_file = create_test_csv();

        let config = CsvReaderConfig {
            delimiter: Some(b','),
            encoding: Some("UTF-8".to_string()),
            has_header: true,
            ..Default::default()
        };

        let mut reader = CsvStreamReader::with_config(temp_file.path(), config).unwrap();
        reader.init().unwrap();

        // Check headers
        let expected_headers = vec!["name".to_string(), "age".to_string(), "city".to_string()];
        assert_eq!(reader.headers(), Some(expected_headers.as_slice()));

        // Read first record
        let record1 = reader.read_record().unwrap().unwrap();
        assert_eq!(record1.fields, vec!["Alice", "30", "NYC"]);

        // Read second record
        let record2 = reader.read_record().unwrap().unwrap();
        assert_eq!(record2.fields, vec!["Bob", "25", "LA"]);

        // Check progress
        let progress = reader.progress();
        assert_eq!(progress.rows_read, 2);
    }

    #[test]
    fn test_csv_batch_reading() {
        let temp_file = create_test_csv();

        let config = CsvReaderConfig {
            delimiter: Some(b','),
            has_header: true,
            ..Default::default()
        };

        let mut reader = CsvStreamReader::with_config(temp_file.path(), config).unwrap();
        reader.init().unwrap();

        let batch = reader.read_batch(2).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].fields[0], "Alice");
        assert_eq!(batch[1].fields[0], "Bob");
    }

    #[test]
    fn test_count_rows() {
        let temp_file = create_test_csv();
        let count = count_rows(temp_file.path(), true).unwrap();
        assert_eq!(count, 3); // 3 data rows (excluding header)
    }
}
