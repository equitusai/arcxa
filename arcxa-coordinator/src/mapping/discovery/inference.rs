//! # Type Inference Engine
//!
//! Intelligent semantic type detection using:
//! - Pattern recognition (regex-based)
//! - Field name heuristics
//! - Data type analysis
//! - Statistical profiling
//!
//! ## Performance
//!
//! Target: <10ms per column
//! - Regex compilation cached
//! - Sample-based inference (not full table scan)
//! - Confidence scoring based on multiple signals

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use tracing::debug;

use super::types::*;

/// Type inference engine
///
/// Infers semantic types from column metadata and sample values.
/// Uses multiple signals:
/// 1. Field name heuristics (e.g., "email" → email type)
/// 2. Pattern detection (e.g., "john@example.com" → email)
/// 3. Data type analysis (e.g., VARCHAR(255) + email pattern → email)
/// 4. Statistical profiling (e.g., high cardinality → identifier)
pub struct TypeInferenceEngine {
    /// Minimum confidence threshold for type inference
    min_confidence: f64,

    /// Sample size for pattern detection
    sample_size: usize,
}

impl TypeInferenceEngine {
    /// Create a new type inference engine
    pub fn new() -> Self {
        Self {
            min_confidence: 0.6,
            sample_size: 100,
        }
    }

    /// Create with custom configuration
    pub fn with_config(min_confidence: f64, sample_size: usize) -> Self {
        Self {
            min_confidence,
            sample_size,
        }
    }

    /// Infer semantic type for a column
    ///
    /// Combines multiple signals:
    /// - Field name heuristics
    /// - Pattern detection on sample values
    /// - Data type analysis
    /// - Statistical profiling
    ///
    /// Returns inferred type with confidence score.
    pub fn infer_type(
        &self,
        column: &ColumnMetadata,
        sample_values: &[String],
        statistics: &ColumnStats,
    ) -> Result<InferenceResult> {
        debug!(
            "Inferring type for column: {} (type: {}, samples: {})",
            column.name,
            column.data_type,
            sample_values.len()
        );

        // Skip if all values are null
        if sample_values.is_empty() {
            return Ok(InferenceResult {
                semantic_type: None,
                confidence: 0.0,
                detected_patterns: vec![],
                statistics: ColumnStatistics::default(),
            });
        }

        // Collect all detected patterns
        let mut patterns = Vec::new();

        // 1. Pattern detection on sample values
        let email_match_rate = self.detect_email_pattern(sample_values);
        if email_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "email".to_string(),
                match_rate: email_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| EMAIL_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        let phone_match_rate = self.detect_phone_pattern(sample_values);
        if phone_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "phone".to_string(),
                match_rate: phone_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| PHONE_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        let uuid_match_rate = self.detect_uuid_pattern(sample_values);
        if uuid_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "uuid".to_string(),
                match_rate: uuid_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| UUID_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        let ssn_match_rate = self.detect_ssn_pattern(sample_values);
        if ssn_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "ssn".to_string(),
                match_rate: ssn_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| SSN_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        let url_match_rate = self.detect_url_pattern(sample_values);
        if url_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "url".to_string(),
                match_rate: url_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| URL_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        let date_match_rate = self.detect_date_pattern(sample_values);
        if date_match_rate > 0.0 {
            patterns.push(DetectedPattern {
                pattern_type: "date".to_string(),
                match_rate: date_match_rate,
                example: sample_values
                    .iter()
                    .find(|v| DATE_PATTERN.is_match(v))
                    .cloned(),
            });
        }

        // 2. Field name heuristics
        let name_hints = self.detect_name_hints(&column.name);

        // 3. Determine semantic type with confidence
        let (semantic_type, confidence) = self.determine_semantic_type(
            &column.name,
            &column.data_type,
            &patterns,
            &name_hints,
            statistics,
        );

        // 4. Build column statistics
        let col_stats = ColumnStatistics {
            distinct_count: statistics.distinct_count,
            null_fraction: statistics.null_fraction,
            sample_count: sample_values.len(),
            most_common_values: statistics
                .most_common_values
                .as_ref()
                .map(|s| s.split(',').take(5).map(|v| v.trim().to_string()).collect()),
            avg_length: Some(self.calculate_avg_length(sample_values)),
            min_value: sample_values.iter().min().cloned(),
            max_value: sample_values.iter().max().cloned(),
        };

        Ok(InferenceResult {
            semantic_type: if confidence >= self.min_confidence {
                semantic_type
            } else {
                None
            },
            confidence,
            detected_patterns: patterns,
            statistics: col_stats,
        })
    }

    // ========================================================================
    // Pattern Detection Methods
    // ========================================================================

    fn detect_email_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| EMAIL_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    fn detect_phone_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| PHONE_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    fn detect_uuid_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| UUID_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    fn detect_ssn_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| SSN_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    fn detect_url_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| URL_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    fn detect_date_pattern(&self, values: &[String]) -> f64 {
        let matches = values.iter().filter(|v| DATE_PATTERN.is_match(v)).count();
        matches as f64 / values.len() as f64
    }

    // ========================================================================
    // Name Heuristics
    // ========================================================================

    fn detect_name_hints(&self, field_name: &str) -> Vec<String> {
        let lower = field_name.to_lowercase();
        let mut hints = Vec::new();

        // Email hints
        if lower.contains("email") || lower.contains("e_mail") {
            hints.push("email".to_string());
        }

        // Phone hints
        if lower.contains("phone") || lower.contains("tel") || lower.contains("mobile") {
            hints.push("phone".to_string());
        }

        // Name hints
        if lower.contains("name") || lower.contains("full_name") {
            hints.push("person_name".to_string());
        }
        if lower.contains("first_name") || lower.contains("fname") {
            hints.push("first_name".to_string());
        }
        if lower.contains("last_name") || lower.contains("lname") || lower.contains("surname") {
            hints.push("last_name".to_string());
        }

        // Identifier hints
        if lower.contains("id") || lower == "uuid" || lower == "guid" {
            hints.push("identifier".to_string());
        }

        // Address hints
        if lower.contains("address") || lower.contains("street") || lower.contains("city") {
            hints.push("address".to_string());
        }
        if lower.contains("zip") || lower.contains("postal") {
            hints.push("postal_code".to_string());
        }
        if lower.contains("country") {
            hints.push("country".to_string());
        }

        // Date/time hints
        if lower.contains("date") || lower.contains("time") || lower.contains("timestamp") {
            hints.push("datetime".to_string());
        }

        // Gender hints
        if lower.contains("gender") || lower.contains("sex") {
            hints.push("gender".to_string());
        }

        // Age hints
        if lower == "age" || lower.contains("age_") {
            hints.push("age".to_string());
        }

        hints
    }

    // ========================================================================
    // Semantic Type Determination
    // ========================================================================

    fn determine_semantic_type(
        &self,
        field_name: &str,
        data_type: &str,
        patterns: &[DetectedPattern],
        name_hints: &[String],
        statistics: &ColumnStats,
    ) -> (Option<String>, f64) {
        let mut scores: Vec<(String, f64)> = Vec::new();

        // Score based on pattern detection (high weight)
        for pattern in patterns {
            let mut score = pattern.match_rate * 0.7; // 70% weight for pattern match

            // Boost score if name hint matches pattern
            if name_hints.contains(&pattern.pattern_type) {
                score += 0.2; // 20% boost for name hint match
            }

            scores.push((pattern.pattern_type.clone(), score));
        }

        // Score based on name hints alone (lower weight)
        for hint in name_hints {
            // Only add if not already scored by pattern
            if !patterns.iter().any(|p| p.pattern_type == *hint) {
                let score = 0.5; // 50% confidence for name hint only
                scores.push((hint.clone(), score));
            }
        }

        // Score based on data type + statistics
        let type_hint = self.infer_from_data_type(data_type, statistics);
        if let Some((type_name, type_score)) = type_hint {
            // Only add if not already scored
            if !scores.iter().any(|(t, _)| t == &type_name) {
                scores.push((type_name, type_score));
            }
        }

        // Find highest scoring type
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

        if let Some((semantic_type, confidence)) = scores.first() {
            debug!(
                "Inferred type for {}: {} (confidence: {:.2})",
                field_name, semantic_type, confidence
            );
            (Some(semantic_type.clone()), *confidence)
        } else {
            (None, 0.0)
        }
    }

    fn infer_from_data_type(
        &self,
        data_type: &str,
        statistics: &ColumnStats,
    ) -> Option<(String, f64)> {
        let lower = data_type.to_lowercase();

        // High cardinality suggests identifier
        if statistics.distinct_count > 1000 && statistics.null_fraction < 0.1 {
            return Some(("identifier".to_string(), 0.6));
        }

        // Low cardinality suggests categorical
        if statistics.distinct_count < 20 {
            return Some(("category".to_string(), 0.5));
        }

        // Date/time types
        if lower.contains("date") || lower.contains("timestamp") || lower.contains("time") {
            return Some(("datetime".to_string(), 0.7));
        }

        // Boolean types
        if lower.contains("bool") || lower == "bit" {
            return Some(("boolean".to_string(), 0.8));
        }

        // Numeric types
        if lower.contains("int") || lower.contains("bigint") || lower.contains("smallint") {
            return Some(("integer".to_string(), 0.7));
        }
        if lower.contains("decimal")
            || lower.contains("numeric")
            || lower.contains("float")
            || lower.contains("double")
        {
            return Some(("decimal".to_string(), 0.7));
        }

        None
    }

    // ========================================================================
    // Statistical Helpers
    // ========================================================================

    fn calculate_avg_length(&self, values: &[String]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        let total: usize = values.iter().map(|v| v.len()).sum();
        total as f64 / values.len() as f64
    }
}

impl Default for TypeInferenceEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Regex Patterns (Lazy-initialized)
// ============================================================================

static EMAIL_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap());

static PHONE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\+?[1-9]\d{1,14}$|^\(\d{3}\)\s?\d{3}-?\d{4}$|^\d{3}-\d{3}-\d{4}$").unwrap()
});

static UUID_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .unwrap()
});

static SSN_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap());

static URL_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap());

static DATE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}$|^\d{2}/\d{2}/\d{4}$|^\d{2}-\d{2}-\d{4}$").unwrap()
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_detection() {
        let engine = TypeInferenceEngine::new();
        let samples = vec![
            "john@example.com".to_string(),
            "jane.doe@company.co.uk".to_string(),
            "user+tag@domain.org".to_string(),
        ];

        let match_rate = engine.detect_email_pattern(&samples);
        assert!(match_rate > 0.9, "Expected high email match rate");
    }

    #[test]
    fn test_phone_detection() {
        let engine = TypeInferenceEngine::new();
        let samples = vec![
            "+14155552671".to_string(),
            "(415) 555-2671".to_string(),
            "415-555-2671".to_string(),
        ];

        let match_rate = engine.detect_phone_pattern(&samples);
        assert!(match_rate > 0.9, "Expected high phone match rate");
    }

    #[test]
    fn test_uuid_detection() {
        let engine = TypeInferenceEngine::new();
        let samples = vec![
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string(),
        ];

        let match_rate = engine.detect_uuid_pattern(&samples);
        assert!(match_rate > 0.9, "Expected high UUID match rate");
    }

    #[test]
    fn test_name_hints() {
        let engine = TypeInferenceEngine::new();

        let hints = engine.detect_name_hints("customer_email");
        assert!(hints.contains(&"email".to_string()));

        let hints = engine.detect_name_hints("phone_number");
        assert!(hints.contains(&"phone".to_string()));

        let hints = engine.detect_name_hints("user_id");
        assert!(hints.contains(&"identifier".to_string()));
    }

    #[test]
    fn test_full_inference() {
        let engine = TypeInferenceEngine::new();

        let column = ColumnMetadata {
            name: "customer_email".to_string(),
            data_type: "VARCHAR(255)".to_string(),
            nullable: true,
            default_value: None,
            primary_key: false,
        };

        let samples = vec![
            "john@example.com".to_string(),
            "jane@company.org".to_string(),
            "user@domain.net".to_string(),
        ];

        let stats = ColumnStats {
            distinct_count: 3,
            null_fraction: 0.0,
            most_common_values: None,
        };

        let result = engine.infer_type(&column, &samples, &stats).unwrap();

        assert_eq!(result.semantic_type, Some("email".to_string()));
        assert!(
            result.confidence > 0.8,
            "Expected high confidence for email"
        );
        assert!(!result.detected_patterns.is_empty());
    }
}
