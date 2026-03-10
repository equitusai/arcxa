//! DB2 DEL File Generator
//!
//! Generates DB2 DEL (delimited) format files optimized for the LOAD utility.
//!
//! ## DEL Format
//!
//! DB2 DEL format is a delimited text format similar to CSV but with specific
//! escaping rules and NULL handling:
//!
//! ```text
//! "value1"|123|"value with \"quotes\""|NULL|"2024-01-15"
//! "value2"|456|"normal value"|"text"|"2024-01-16"
//! ```
//!
//! ## Features
//!
//! - Streaming to file (no memory buffering)
//! - DB2-specific escaping (quotes, delimiters, newlines)
//! - NULL value handling
//! - Column mapping with transformations
//! - Progress tracking
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphica_coordinator::mapping::loader::db2_del_generator::{DelFileGenerator, DelGeneratorConfig};
//!
//! let config = DelGeneratorConfig {
//!     output_path: PathBuf::from("/tmp/customers.del"),
//!     delimiter: '|',
//!     null_indicator: "NULL".to_string(),
//!     ..Default::default()
//! };
//!
//! let mut generator = DelFileGenerator::new(config)?;
//!
//! // Write rows
//! generator.write_row(&["Alice", "30", "alice@example.com"])?;
//! generator.write_row(&["Bob", "25", "bob@example.com"])?;
//!
//! // Finalize and get statistics
//! let stats = generator.finalize()?;
//! println!("Wrote {} rows, {} bytes", stats.rows_written, stats.bytes_written);
//! ```

use anyhow::{Context, Result};
use csv::StringRecord;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// DEL file generator configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelGeneratorConfig {
    /// Output file path
    pub output_path: PathBuf,

    /// Delimiter character (default: |)
    pub delimiter: char,

    /// NULL indicator string (default: "NULL")
    pub null_indicator: String,

    /// Quote character (default: ")
    pub quote: char,

    /// Escape character for quotes (default: ")
    pub escape: char,

    /// Buffer size for writing (default: 8MB)
    pub buffer_size: usize,

    /// Column mappings (source index → target index)
    pub column_mappings: Vec<ColumnMapping>,
}

impl Default for DelGeneratorConfig {
    fn default() -> Self {
        Self {
            output_path: PathBuf::from("/tmp/output.del"),
            delimiter: '|',
            null_indicator: "NULL".to_string(),
            quote: '"',
            escape: '"',
            buffer_size: 8 * 1024 * 1024, // 8MB
            column_mappings: Vec::new(),
        }
    }
}

/// Column mapping with optional transformation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMapping {
    /// Source column index (0-based)
    pub source_index: usize,

    /// Target column index (0-based)
    pub target_index: usize,

    /// Optional transformation expression
    pub transformation: Option<String>,

    /// Whether NULL values are allowed
    pub nullable: bool,

    /// Default value if source is NULL
    pub default_value: Option<String>,
}

/// DEL file generation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelFileStats {
    /// Number of rows written
    pub rows_written: u64,

    /// Total bytes written
    pub bytes_written: u64,

    /// Number of NULL values encountered
    pub null_values_count: u64,

    /// Number of escaped characters
    pub escaped_chars_count: u64,

    /// Number of errors/warnings
    pub errors: Vec<DelGenerationError>,
}

/// DEL generation error/warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelGenerationError {
    /// Row number (1-indexed)
    pub row_number: u64,

    /// Column index
    pub column_index: usize,

    /// Error message
    pub message: String,

    /// Whether this is a warning (vs fatal error)
    pub is_warning: bool,
}

/// DEL file generator (streaming)
pub struct DelFileGenerator {
    /// Configuration
    config: DelGeneratorConfig,

    /// Output file writer
    writer: BufWriter<File>,

    /// Statistics
    rows_written: u64,
    bytes_written: u64,
    null_values_count: u64,
    escaped_chars_count: u64,

    /// Errors/warnings
    errors: Vec<DelGenerationError>,
}

impl DelFileGenerator {
    /// Create new DEL file generator
    pub fn new(config: DelGeneratorConfig) -> Result<Self> {
        // Create output file
        let file = File::create(&config.output_path).with_context(|| {
            format!("Failed to create DEL output file: {:?}", config.output_path)
        })?;

        let writer = BufWriter::with_capacity(config.buffer_size, file);

        Ok(Self {
            config,
            writer,
            rows_written: 0,
            bytes_written: 0,
            null_values_count: 0,
            escaped_chars_count: 0,
            errors: Vec::new(),
        })
    }

    /// Write a row to DEL file
    pub fn write_row(&mut self, row: &StringRecord) -> Result<()> {
        self.rows_written += 1;

        // Apply column mappings if specified
        let values: Vec<String> = if self.config.column_mappings.is_empty() {
            // No mappings - write all columns as-is
            row.iter().map(|s| s.to_string()).collect()
        } else {
            // Apply mappings
            let mut mapped_values = vec![String::new(); self.config.column_mappings.len()];

            for mapping in &self.config.column_mappings {
                let source_value = row.get(mapping.source_index);

                let value = if let Some(val) = source_value {
                    if val.is_empty() && !mapping.nullable {
                        // Empty non-nullable - use default or error
                        if let Some(default) = &mapping.default_value {
                            default.clone()
                        } else {
                            self.errors.push(DelGenerationError {
                                row_number: self.rows_written,
                                column_index: mapping.source_index,
                                message: "Non-nullable column has empty value".to_string(),
                                is_warning: true,
                            });
                            self.config.null_indicator.clone()
                        }
                    } else {
                        val.to_string()
                    }
                } else {
                    // Column not found in source
                    if let Some(default) = &mapping.default_value {
                        default.clone()
                    } else {
                        self.config.null_indicator.clone()
                    }
                };

                mapped_values[mapping.target_index] = value;
            }

            mapped_values
        };

        // Write formatted row
        for (i, value) in values.iter().enumerate() {
            if i > 0 {
                write!(self.writer, "{}", self.config.delimiter)?;
                self.bytes_written += 1;
            }

            let formatted = self.format_value(value)?;
            write!(self.writer, "{}", formatted)?;
            self.bytes_written += formatted.len() as u64;
        }

        // Write newline
        writeln!(self.writer)?;
        self.bytes_written += 1;

        Ok(())
    }

    /// Write a row from raw strings
    pub fn write_row_strings(&mut self, values: &[&str]) -> Result<()> {
        let record: StringRecord = values.iter().map(|s| *s).collect();
        self.write_row(&record)
    }

    /// Format a value for DEL file
    fn format_value(&mut self, value: &str) -> Result<String> {
        // Check if value is NULL
        if value.is_empty() || value == &self.config.null_indicator {
            self.null_values_count += 1;
            return Ok(self.config.null_indicator.clone());
        }

        // Quote and escape the value
        let mut result = String::with_capacity(value.len() + 10);
        result.push(self.config.quote);

        for ch in value.chars() {
            if ch == self.config.quote {
                // Escape quote with double quote (DB2 convention)
                result.push(self.config.escape);
                result.push(ch);
                self.escaped_chars_count += 1;
            } else if ch == '\n' {
                // Keep newlines as-is inside quoted strings (DB2 handles this)
                result.push(ch);
            } else if ch == '\r' {
                // Skip carriage returns
                continue;
            } else {
                result.push(ch);
            }
        }

        result.push(self.config.quote);

        Ok(result)
    }

    /// Flush buffer to disk
    pub fn flush(&mut self) -> Result<()> {
        self.writer.flush()?;
        Ok(())
    }

    /// Finalize and get statistics
    pub fn finalize(mut self) -> Result<DelFileStats> {
        self.writer.flush()?;

        Ok(DelFileStats {
            rows_written: self.rows_written,
            bytes_written: self.bytes_written,
            null_values_count: self.null_values_count,
            escaped_chars_count: self.escaped_chars_count,
            errors: self.errors,
        })
    }

    /// Get current statistics (without finalizing)
    pub fn stats(&self) -> DelFileStats {
        DelFileStats {
            rows_written: self.rows_written,
            bytes_written: self.bytes_written,
            null_values_count: self.null_values_count,
            escaped_chars_count: self.escaped_chars_count,
            errors: self.errors.clone(),
        }
    }

    /// Get output file path
    pub fn output_path(&self) -> &Path {
        &self.config.output_path
    }

    /// Get rows written
    pub fn rows_written(&self) -> u64 {
        self.rows_written
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use tempfile::NamedTempFile;

    fn read_file_contents(path: &Path) -> String {
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        contents
    }

    #[test]
    fn test_simple_del_generation() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            delimiter: '|',
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        generator.write_row_strings(&["Alice", "30", "alice@example.com"])?;
        generator.write_row_strings(&["Bob", "25", "bob@example.com"])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 2);
        assert!(stats.bytes_written > 0);

        // Verify file contents
        let contents = read_file_contents(&output_path);
        assert!(contents.contains("Alice"));
        assert!(contents.contains("Bob"));
        assert!(contents.contains("|"));

        Ok(())
    }

    #[test]
    fn test_null_values() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            null_indicator: "NULL".to_string(),
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        generator.write_row_strings(&["Alice", "", "alice@example.com"])?;
        generator.write_row_strings(&["Bob", "25", ""])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 2);
        assert_eq!(stats.null_values_count, 2); // Two empty values

        // Verify NULL appears in file
        let contents = read_file_contents(&output_path);
        assert!(contents.contains("NULL"));

        Ok(())
    }

    #[test]
    fn test_quoted_values() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            delimiter: '|',
            quote: '"',
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        generator.write_row_strings(&["Alice", "Has a \"quote\"", "test"])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 1);
        assert_eq!(stats.escaped_chars_count, 2); // Two escaped quotes

        // Verify escaped quotes in file
        let contents = read_file_contents(&output_path);
        assert!(contents.contains(r#""""#)); // Double quote for escaping

        Ok(())
    }

    #[test]
    fn test_delimiter_in_value() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            delimiter: '|',
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        generator.write_row_strings(&["Alice", "Has a | delimiter", "test"])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 1);

        // Verify delimiter inside quotes is preserved
        let contents = read_file_contents(&output_path);
        assert!(contents.contains("Has a | delimiter"));

        Ok(())
    }

    #[test]
    fn test_column_mapping() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            delimiter: '|',
            column_mappings: vec![
                ColumnMapping {
                    source_index: 0,
                    target_index: 1,
                    transformation: None,
                    nullable: true,
                    default_value: None,
                },
                ColumnMapping {
                    source_index: 1,
                    target_index: 0,
                    transformation: None,
                    nullable: true,
                    default_value: None,
                },
            ],
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        // Source: ["Alice", "30"]
        // Target (swapped): ["30", "Alice"]
        generator.write_row_strings(&["Alice", "30"])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 1);

        // Verify columns are swapped
        let contents = read_file_contents(&output_path);
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("\"30\""));
        assert!(lines[0].ends_with("\"Alice\""));

        Ok(())
    }

    #[test]
    fn test_default_values() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path: output_path.clone(),
            column_mappings: vec![
                ColumnMapping {
                    source_index: 0,
                    target_index: 0,
                    transformation: None,
                    nullable: true,
                    default_value: None,
                },
                ColumnMapping {
                    source_index: 1,
                    target_index: 1,
                    transformation: None,
                    nullable: false,
                    default_value: Some("UNKNOWN".to_string()),
                },
            ],
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        // Second column empty - should use default
        generator.write_row_strings(&["Alice", ""])?;

        let stats = generator.finalize()?;

        assert_eq!(stats.rows_written, 1);

        // Verify default value was used
        let contents = read_file_contents(&output_path);
        assert!(contents.contains("UNKNOWN"));

        Ok(())
    }

    #[test]
    fn test_progress_tracking() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let output_path = temp_file.path().to_path_buf();

        let config = DelGeneratorConfig {
            output_path,
            ..Default::default()
        };

        let mut generator = DelFileGenerator::new(config)?;

        // Write multiple rows and check progress
        for i in 0..10 {
            generator.write_row_strings(&["Alice", &i.to_string()])?;
            assert_eq!(generator.rows_written(), i + 1);
        }

        let stats = generator.stats();
        assert_eq!(stats.rows_written, 10);
        assert!(stats.bytes_written > 0);

        Ok(())
    }
}
