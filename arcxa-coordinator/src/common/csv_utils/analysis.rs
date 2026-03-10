//! CSV Analysis Layer
//!
//! Schema inference, PII detection, quality profiling, and type inference
//! for CSV files. Used primarily by the File Library scanner for metadata
//! discovery.

use anyhow::Result;
use regex::Regex;
use std::collections::{HashMap, HashSet};

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for schema inference
#[derive(Debug, Clone)]
pub struct SchemaInferenceConfig {
    /// Number of rows to sample for type inference
    pub sample_rows: usize,

    /// Confidence threshold for type inference (0.0 - 1.0)
    pub type_confidence_threshold: f64,

    /// Enable PII detection
    pub enable_pii_detection: bool,

    /// Enable quality profiling
    pub enable_quality_profiling: bool,

    /// Track unique values for cardinality (up to this limit)
    pub max_unique_values_tracked: usize,
}

impl Default for SchemaInferenceConfig {
    fn default() -> Self {
        Self {
            sample_rows: 1000,
            type_confidence_threshold: 0.7,
            enable_pii_detection: true,
            enable_quality_profiling: true,
            max_unique_values_tracked: 100,
        }
    }
}

// ============================================================================
// Types
// ============================================================================

/// Field data type with confidence
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InferredFieldType {
    String,
    Integer,
    Float,
    Boolean,
    Timestamp,
    Date,
    Email,
    Phone,
    Ssn,
    CreditCard,
    Url,
    IpAddress,
    Json,
    Xml,
}

/// Inferred field schema
#[derive(Debug, Clone)]
pub struct InferredField {
    pub name: String,
    pub field_type: InferredFieldType,
    pub type_confidence: f64,
    pub nullable: bool,
    pub null_count: usize,
    pub sample_values: Vec<String>,
    pub unique_count: Option<usize>,
    pub is_pii: bool,
    pub pii_types: Vec<PiiType>,
}

/// PII type classification
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PiiType {
    Email,
    Phone,
    Ssn,
    CreditCard,
    Custom(String),
}

/// Quality metrics for a field
#[derive(Debug, Clone)]
pub struct FieldQualityMetrics {
    pub field_name: String,
    pub null_rate: f64,
    pub empty_rate: f64,
    pub distinct_count: usize,
    pub distinct_rate: f64,
    pub duplicate_count: usize,
}

// ============================================================================
// Header Detection
// ============================================================================

/// Detect if CSV has header row using multiple heuristics
pub struct HeaderDetection;

impl HeaderDetection {
    /// Detect header with evidence-based scoring
    pub fn detect_header(rows: &[Vec<String>], _delimiter: &str) -> Result<(bool, f64)> {
        if rows.len() < 2 {
            return Ok((false, 0.0));
        }

        let first_row = &rows[0];
        let data_rows = &rows[1..std::cmp::min(20, rows.len())];

        let mut evidence_score = 0.0;

        // Heuristic 1: First row has unique values (likely column names)
        let first_row_unique = first_row.iter().collect::<HashSet<_>>().len() == first_row.len();
        if first_row_unique {
            evidence_score += 2.0;
        }

        // Heuristic 2: First row is all text, data rows have numbers
        let first_all_text = first_row
            .iter()
            .all(|v| v.parse::<f64>().is_err() && !v.is_empty());
        let data_has_numbers = data_rows
            .iter()
            .flat_map(|row| row.iter())
            .any(|v| v.parse::<f64>().is_ok());

        if first_all_text && data_has_numbers {
            evidence_score += 3.0;
        }

        // Heuristic 3: First row has typical header patterns
        let has_header_patterns = first_row.iter().any(|v| {
            v.contains('_')
                || v.chars().any(|c| c.is_uppercase())
                || v.to_lowercase().contains("name")
                || v.to_lowercase().contains("id")
                || v.to_lowercase().contains("date")
                || v.to_lowercase().contains("time")
        });
        if has_header_patterns {
            evidence_score += 1.5;
        }

        // Heuristic 4: Type consistency in columns
        if data_rows.len() > 5 {
            let mut consistent_types = 0;
            for col_idx in 0..first_row.len() {
                let column_values: Vec<String> = data_rows
                    .iter()
                    .filter_map(|row| row.get(col_idx).cloned())
                    .collect();

                if Self::has_consistent_type(&column_values) {
                    consistent_types += 1;
                }
            }
            let consistency_ratio = consistent_types as f64 / first_row.len() as f64;
            if consistency_ratio > 0.7 {
                evidence_score += 2.0;
            }
        }

        // Decision: threshold for header detection
        let has_header = evidence_score >= 3.0;
        let confidence = (evidence_score / 8.5_f64).min(1.0_f64); // Max score is 8.5

        Ok((has_header, confidence))
    }

    fn has_consistent_type(values: &[String]) -> bool {
        let mut type_counts: HashMap<String, usize> = HashMap::new();

        for value in values {
            let detected_type = if value.parse::<i64>().is_ok() {
                "integer"
            } else if value.parse::<f64>().is_ok() {
                "float"
            } else if value.parse::<bool>().is_ok() {
                "boolean"
            } else {
                "string"
            };

            *type_counts.entry(detected_type.to_string()).or_insert(0) += 1;
        }

        // Type is consistent if one type dominates (>70%)
        if let Some(max_count) = type_counts.values().max() {
            let ratio = *max_count as f64 / values.len() as f64;
            ratio > 0.7
        } else {
            false
        }
    }
}

// ============================================================================
// Type Inference
// ============================================================================

/// Field type inference engine
pub struct FieldTypeInference {
    config: SchemaInferenceConfig,
}

impl FieldTypeInference {
    pub fn new(config: SchemaInferenceConfig) -> Self {
        Self { config }
    }

    /// Infer field types from sample values
    pub fn infer_field_type(&self, values: &[String]) -> (InferredFieldType, f64) {
        if values.is_empty() {
            return (InferredFieldType::String, 0.0);
        }

        let mut type_scores: HashMap<InferredFieldType, usize> = HashMap::new();

        for value in values {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Test each type
            if Self::is_integer(trimmed) {
                *type_scores.entry(InferredFieldType::Integer).or_insert(0) += 1;
            }
            if Self::is_float(trimmed) {
                *type_scores.entry(InferredFieldType::Float).or_insert(0) += 1;
            }
            if Self::is_boolean(trimmed) {
                *type_scores.entry(InferredFieldType::Boolean).or_insert(0) += 1;
            }
            if Self::is_timestamp(trimmed) {
                *type_scores.entry(InferredFieldType::Timestamp).or_insert(0) += 1;
            }
            if Self::is_date(trimmed) {
                *type_scores.entry(InferredFieldType::Date).or_insert(0) += 1;
            }
            if Self::is_email(trimmed) {
                *type_scores.entry(InferredFieldType::Email).or_insert(0) += 1;
            }
            if Self::is_phone(trimmed) {
                *type_scores.entry(InferredFieldType::Phone).or_insert(0) += 1;
            }
            if Self::is_ssn(trimmed) {
                *type_scores.entry(InferredFieldType::Ssn).or_insert(0) += 1;
            }
            if Self::is_url(trimmed) {
                *type_scores.entry(InferredFieldType::Url).or_insert(0) += 1;
            }
            if Self::is_ip_address(trimmed) {
                *type_scores.entry(InferredFieldType::IpAddress).or_insert(0) += 1;
            }
            if Self::is_json(trimmed) {
                *type_scores.entry(InferredFieldType::Json).or_insert(0) += 1;
            }
            if Self::is_xml(trimmed) {
                *type_scores.entry(InferredFieldType::Xml).or_insert(0) += 1;
            }

            // Always counts as string
            *type_scores.entry(InferredFieldType::String).or_insert(0) += 1;
        }

        // Find type with highest score
        let total_non_empty = values.iter().filter(|v| !v.trim().is_empty()).count();

        let mut best_type = InferredFieldType::String;
        let mut best_confidence = 0.0;

        for (field_type, count) in type_scores.iter() {
            // Skip string as default fallback
            if *field_type == InferredFieldType::String {
                continue;
            }

            let confidence = *count as f64 / total_non_empty as f64;
            if confidence > best_confidence && confidence >= self.config.type_confidence_threshold {
                best_type = field_type.clone();
                best_confidence = confidence;
            }
        }

        // If no specific type matched, use String
        if best_confidence == 0.0 {
            best_confidence = 1.0;
            best_type = InferredFieldType::String;
        }

        (best_type, best_confidence)
    }

    // Type checking functions
    fn is_integer(value: &str) -> bool {
        value.parse::<i64>().is_ok()
    }

    fn is_float(value: &str) -> bool {
        value.parse::<f64>().is_ok() && !value.parse::<i64>().is_ok()
    }

    fn is_boolean(value: &str) -> bool {
        matches!(
            value.to_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "1" | "0" | "t" | "f" | "y" | "n"
        )
    }

    fn is_timestamp(value: &str) -> bool {
        // ISO 8601 timestamp
        chrono::DateTime::parse_from_rfc3339(value).is_ok()
            || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok()
            || chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").is_ok()
    }

    fn is_date(value: &str) -> bool {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
            || chrono::NaiveDate::parse_from_str(value, "%m/%d/%Y").is_ok()
            || chrono::NaiveDate::parse_from_str(value, "%d/%m/%Y").is_ok()
    }

    fn is_email(value: &str) -> bool {
        // RFC 5322 simplified
        let re = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
        re.is_match(value)
    }

    fn is_phone(value: &str) -> bool {
        // International and US formats
        let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.len() >= 10 && digits.len() <= 15
    }

    fn is_ssn(value: &str) -> bool {
        let re = Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap();
        re.is_match(value)
    }

    fn is_url(value: &str) -> bool {
        value.starts_with("http://") || value.starts_with("https://") || value.starts_with("ftp://")
    }

    fn is_ip_address(value: &str) -> bool {
        // IPv4
        let re = Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").unwrap();
        re.is_match(value)
    }

    fn is_json(value: &str) -> bool {
        value.starts_with('{') && value.ends_with('}')
            || value.starts_with('[') && value.ends_with(']')
    }

    fn is_xml(value: &str) -> bool {
        value.starts_with('<') && value.ends_with('>')
    }
}

// ============================================================================
// PII Detection
// ============================================================================

/// PII detection engine with validation
pub struct PiiDetection;

impl PiiDetection {
    /// Detect PII in field values
    pub fn detect_pii(values: &[String]) -> Vec<PiiType> {
        let mut detected_types = HashSet::new();

        for value in values {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }

            if Self::is_email(trimmed) {
                detected_types.insert(PiiType::Email);
            }
            if Self::is_phone(trimmed) {
                detected_types.insert(PiiType::Phone);
            }
            if Self::is_ssn(trimmed) {
                detected_types.insert(PiiType::Ssn);
            }
            if Self::is_credit_card(trimmed) {
                detected_types.insert(PiiType::CreditCard);
            }
        }

        detected_types.into_iter().collect()
    }

    fn is_email(value: &str) -> bool {
        let re = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
        re.is_match(value)
    }

    fn is_phone(value: &str) -> bool {
        let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
        digits.len() >= 10 && digits.len() <= 15
    }

    fn is_ssn(value: &str) -> bool {
        let re = Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap();
        re.is_match(value)
    }

    fn is_credit_card(value: &str) -> bool {
        let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();

        if digits.len() < 13 || digits.len() > 19 {
            return false;
        }

        // Luhn algorithm validation
        Self::luhn_check(&digits)
    }

    /// Luhn algorithm for credit card validation
    fn luhn_check(digits: &str) -> bool {
        let mut sum = 0;
        let mut alternate = false;

        for digit_char in digits.chars().rev() {
            if let Some(mut digit) = digit_char.to_digit(10) {
                if alternate {
                    digit *= 2;
                    if digit > 9 {
                        digit -= 9;
                    }
                }
                sum += digit;
                alternate = !alternate;
            }
        }

        sum % 10 == 0
    }
}

// ============================================================================
// Quality Profiling
// ============================================================================

/// Quality profiling for CSV fields
pub struct QualityProfiler {
    config: SchemaInferenceConfig,
}

impl QualityProfiler {
    pub fn new(config: SchemaInferenceConfig) -> Self {
        Self { config }
    }

    /// Calculate quality metrics for a field
    pub fn profile_field(&self, field_name: &str, values: &[String]) -> FieldQualityMetrics {
        let total_count = values.len();
        let null_count = values.iter().filter(|v| v.is_empty()).count();
        let empty_count = values.iter().filter(|v| v.trim().is_empty()).count();

        let mut unique_values = HashSet::new();
        for value in values {
            if !value.is_empty() {
                unique_values.insert(value.clone());
            }
        }

        let distinct_count = unique_values.len();
        let non_null_count = total_count - null_count;

        let null_rate = if total_count > 0 {
            null_count as f64 / total_count as f64
        } else {
            0.0
        };

        let empty_rate = if total_count > 0 {
            empty_count as f64 / total_count as f64
        } else {
            0.0
        };

        let distinct_rate = if non_null_count > 0 {
            distinct_count as f64 / non_null_count as f64
        } else {
            0.0
        };

        let duplicate_count = non_null_count.saturating_sub(distinct_count);

        FieldQualityMetrics {
            field_name: field_name.to_string(),
            null_rate,
            empty_rate,
            distinct_count,
            distinct_rate,
            duplicate_count,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_inference_integer() {
        let config = SchemaInferenceConfig::default();
        let inference = FieldTypeInference::new(config);

        let values = vec!["1".to_string(), "2".to_string(), "3".to_string()];
        let (field_type, confidence) = inference.infer_field_type(&values);

        assert_eq!(field_type, InferredFieldType::Integer);
        assert!(confidence > 0.9);
    }

    #[test]
    fn test_pii_detection_email() {
        let values = vec!["user@example.com".to_string(), "test@test.org".to_string()];

        let pii_types = PiiDetection::detect_pii(&values);
        assert!(pii_types.contains(&PiiType::Email));
    }

    #[test]
    fn test_luhn_algorithm() {
        // Valid test credit card number
        assert!(PiiDetection::luhn_check("4532015112830366"));

        // Invalid
        assert!(!PiiDetection::luhn_check("1234567890123456"));
    }

    #[test]
    fn test_header_detection() {
        let rows = vec![
            vec!["name".to_string(), "age".to_string(), "email".to_string()],
            vec![
                "Alice".to_string(),
                "30".to_string(),
                "alice@example.com".to_string(),
            ],
            vec![
                "Bob".to_string(),
                "25".to_string(),
                "bob@example.com".to_string(),
            ],
        ];

        let (has_header, confidence) = HeaderDetection::detect_header(&rows, ",").unwrap();
        assert!(has_header);
        assert!(confidence > 0.5);
    }
}
