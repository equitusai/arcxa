//! Core CSV Detection and Parsing Primitives
//!
//! Low-level utilities for CSV format detection and parsing that are used
//! by both the analysis layer (schema inference, PII) and streaming layer
//! (production reads).

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for CSV detection
#[derive(Debug, Clone)]
pub struct CsvDetectionConfig {
    /// Number of lines to sample for detection
    pub sample_lines: usize,

    /// Candidate delimiters to test
    pub candidate_delimiters: Vec<String>,

    /// Minimum confidence threshold (0.0 - 1.0)
    pub min_confidence: f64,

    /// Maximum bytes to read for encoding detection
    pub encoding_sample_bytes: usize,
}

impl Default for CsvDetectionConfig {
    fn default() -> Self {
        Self {
            sample_lines: 50,
            candidate_delimiters: vec![
                ",".to_string(),
                ";".to_string(),
                "\t".to_string(),
                "|".to_string(),
            ],
            min_confidence: 0.5,
            encoding_sample_bytes: 8192,
        }
    }
}

// ============================================================================
// Types
// ============================================================================

/// Detected CSV encoding
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Utf32Be,
    Latin1,
    Windows1252,
}

impl CsvEncoding {
    pub fn as_str(&self) -> &str {
        match self {
            CsvEncoding::Utf8 => "UTF-8",
            CsvEncoding::Utf8Bom => "UTF-8-BOM",
            CsvEncoding::Utf16Le => "UTF-16LE",
            CsvEncoding::Utf16Be => "UTF-16BE",
            CsvEncoding::Utf32Be => "UTF-32BE",
            CsvEncoding::Latin1 => "ISO-8859-1",
            CsvEncoding::Windows1252 => "Windows-1252",
        }
    }
}

/// Delimiter detection result with confidence
#[derive(Debug, Clone)]
pub struct DelimiterCandidate {
    pub delimiter: String,
    pub confidence: f64,
    pub avg_fields_per_row: f64,
    pub field_count_stddev: f64,
    pub consistency_score: f64,
}

// ============================================================================
// Encoding Detection
// ============================================================================

/// Detect file encoding with BOM (Byte Order Mark) support
pub fn detect_encoding_advanced(path: &Path) -> Result<CsvEncoding> {
    let mut file = File::open(path)
        .with_context(|| format!("Failed to open file for encoding detection: {:?}", path))?;

    let mut buffer = vec![0u8; 4];
    let bytes_read = file.read(&mut buffer)?;

    if bytes_read == 0 {
        return Ok(CsvEncoding::Utf8);
    }

    // Check for BOM
    if bytes_read >= 3 && buffer[0] == 0xEF && buffer[1] == 0xBB && buffer[2] == 0xBF {
        return Ok(CsvEncoding::Utf8Bom);
    }

    if bytes_read >= 2 {
        if buffer[0] == 0xFF && buffer[1] == 0xFE {
            return Ok(CsvEncoding::Utf16Le);
        }
        if buffer[0] == 0xFE && buffer[1] == 0xFF {
            return Ok(CsvEncoding::Utf16Be);
        }
    }

    if bytes_read >= 4
        && buffer[0] == 0x00
        && buffer[1] == 0x00
        && buffer[2] == 0xFE
        && buffer[3] == 0xFF
    {
        return Ok(CsvEncoding::Utf32Be);
    }

    // Read more bytes for heuristic detection
    let mut full_buffer = buffer[..bytes_read].to_vec();
    file.read_to_end(&mut full_buffer)?;

    // Limit sample size
    let sample = if full_buffer.len() > 8192 {
        &full_buffer[..8192]
    } else {
        &full_buffer
    };

    // Try UTF-8 validation
    if std::str::from_utf8(sample).is_ok() {
        return Ok(CsvEncoding::Utf8);
    }

    // Heuristic: if high-bit characters present, likely Latin1/Windows-1252
    let high_bit_count = sample.iter().filter(|&&b| b >= 0x80).count();
    if high_bit_count > 0 {
        // Windows-1252 is more common in CSV files
        return Ok(CsvEncoding::Windows1252);
    }

    Ok(CsvEncoding::Utf8)
}

// ============================================================================
// Delimiter Detection
// ============================================================================

/// Detect delimiter using statistical analysis with confidence scoring
pub fn detect_delimiter_advanced(
    path: &Path,
    config: &CsvDetectionConfig,
) -> Result<DelimiterCandidate> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open file for delimiter detection: {:?}", path))?;

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .take(config.sample_lines)
        .filter_map(|line| line.ok())
        .filter(|line| !line.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return Ok(DelimiterCandidate {
            delimiter: ",".to_string(),
            confidence: 0.0,
            avg_fields_per_row: 0.0,
            field_count_stddev: 0.0,
            consistency_score: 0.0,
        });
    }

    // Analyze each candidate delimiter
    let mut candidates: Vec<DelimiterCandidate> = Vec::new();

    for delimiter in &config.candidate_delimiters {
        if let Ok(candidate) = analyze_delimiter(&lines, delimiter) {
            candidates.push(candidate);
        }
    }

    // Sort by confidence (descending)
    candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    // Return best candidate or default
    Ok(candidates
        .into_iter()
        .next()
        .unwrap_or_else(|| DelimiterCandidate {
            delimiter: ",".to_string(),
            confidence: 0.0,
            avg_fields_per_row: 1.0,
            field_count_stddev: 0.0,
            consistency_score: 0.0,
        }))
}

/// Analyze a single delimiter candidate
fn analyze_delimiter(lines: &[String], delimiter: &str) -> Result<DelimiterCandidate> {
    let mut field_counts = Vec::new();

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }

        // Parse the line respecting quotes
        let fields = parse_csv_line_advanced(line, delimiter)?;
        field_counts.push(fields.len());
    }

    if field_counts.is_empty() {
        return Ok(DelimiterCandidate {
            delimiter: delimiter.to_string(),
            confidence: 0.0,
            avg_fields_per_row: 0.0,
            field_count_stddev: 0.0,
            consistency_score: 0.0,
        });
    }

    // Calculate statistics
    let avg = field_counts.iter().sum::<usize>() as f64 / field_counts.len() as f64;
    let variance = field_counts
        .iter()
        .map(|&count| {
            let diff = count as f64 - avg;
            diff * diff
        })
        .sum::<f64>()
        / field_counts.len() as f64;
    let stddev = variance.sqrt();

    // Consistency: low stddev is good
    let consistency = if avg > 1.0 {
        1.0 - (stddev / avg).min(1.0)
    } else {
        0.0
    };

    // Confidence: high average fields + high consistency
    // Fields >= 2 is minimum for CSV
    // Use a more forgiving formula for typical CSVs (2-20 fields)
    let confidence = if avg >= 2.0 {
        // Normalize field count: 2 fields = 0.6, 5 fields = 1.0
        let field_score = ((avg - 1.0) / 4.0).min(1.0).max(0.6);
        field_score * consistency
    } else {
        0.0
    };

    Ok(DelimiterCandidate {
        delimiter: delimiter.to_string(),
        confidence,
        avg_fields_per_row: avg,
        field_count_stddev: stddev,
        consistency_score: consistency,
    })
}

// ============================================================================
// CSV Line Parsing
// ============================================================================

/// Parse a single CSV line with proper quote handling
///
/// Handles:
/// - Quoted fields with embedded delimiters
/// - Escaped quotes (doubled quotes: "")
/// - Whitespace preservation inside quotes
pub fn parse_csv_line_advanced(line: &str, delimiter: &str) -> Result<Vec<String>> {
    let mut fields = Vec::new();
    let mut current_field = String::new();
    let mut in_quotes = false;
    let mut i = 0;
    let chars: Vec<char> = line.chars().collect();

    while i < chars.len() {
        let c = chars[i];

        match c {
            '"' => {
                if in_quotes {
                    // Check for escaped quote (doubled quote)
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        current_field.push('"');
                        i += 2; // Skip both quotes
                    } else {
                        // End of quoted field
                        in_quotes = false;
                        i += 1;
                    }
                } else {
                    // Start of quoted field
                    in_quotes = true;
                    i += 1;
                }
            }
            _ => {
                // Check if we hit delimiter (only if not in quotes)
                if !in_quotes {
                    // Check if current position starts with delimiter
                    let remaining = &line[i..];
                    if remaining.starts_with(delimiter) {
                        fields.push(current_field.clone());
                        current_field.clear();
                        i += delimiter.len();
                        continue;
                    }
                }

                current_field.push(c);
                i += 1;
            }
        }
    }

    // Push last field
    fields.push(current_field);

    Ok(fields)
}

/// Simple CSV line parsing (fallback for non-quoted CSVs)
pub fn parse_csv_line_simple(line: &str, delimiter: &str) -> Vec<String> {
    line.split(delimiter)
        .map(|s| s.trim().to_string())
        .collect()
}

// ============================================================================
// Quote Detection
// ============================================================================

/// Detect if CSV uses quotes for field enclosure
pub fn detect_quote_usage(lines: &[String]) -> bool {
    // Check if any line contains quoted fields
    lines.iter().any(|line| {
        let mut in_quotes = false;
        let mut has_quoted_field = false;

        for c in line.chars() {
            if c == '"' {
                in_quotes = !in_quotes;
                has_quoted_field = true;
            }
        }

        has_quoted_field && !in_quotes // Well-formed quotes
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv_line_simple() {
        let line = "a,b,c";
        let fields = parse_csv_line_simple(line, ",");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_line_with_quotes() {
        let line = r#"a,"b,c",d"#;
        let fields = parse_csv_line_advanced(line, ",").unwrap();
        assert_eq!(fields, vec!["a", "b,c", "d"]);
    }

    #[test]
    fn test_parse_csv_line_with_escaped_quotes() {
        let line = r#"a,"b""c",d"#;
        let fields = parse_csv_line_advanced(line, ",").unwrap();
        assert_eq!(fields, vec!["a", r#"b"c"#, "d"]);
    }

    #[test]
    fn test_delimiter_consistency() {
        let lines = vec![
            "a,b,c".to_string(),
            "1,2,3".to_string(),
            "x,y,z".to_string(),
        ];

        let candidate = analyze_delimiter(&lines, ",").unwrap();
        assert_eq!(candidate.avg_fields_per_row, 3.0);
        assert_eq!(candidate.field_count_stddev, 0.0);
        assert_eq!(candidate.consistency_score, 1.0);
        assert!(candidate.confidence > 0.5);
    }

    #[test]
    fn test_detect_quote_usage() {
        let lines = vec![r#"a,"b,c",d"#.to_string(), r#"1,"2,3",4"#.to_string()];
        assert!(detect_quote_usage(&lines));

        let lines_no_quotes = vec!["a,b,c".to_string(), "1,2,3".to_string()];
        assert!(!detect_quote_usage(&lines_no_quotes));
    }
}
