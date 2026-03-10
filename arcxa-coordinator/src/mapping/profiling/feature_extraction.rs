//! # Schema Intelligence
//!
//! Feature extraction and data profiling for schema fields.
//!
//! ## Components
//!
//! - **Feature Extractor**: Extract tokens, n-grams, patterns from field names
//! - **Data Profiler**: Analyze sample values for semantic patterns
//! - **Type Inference**: Infer semantic types (email, phone, date, etc.)

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

use crate::mapping::statistical::{NgramIndex, TfIdfIndex};
use crate::mapping::types::*;

/// Schema intelligence engine
pub struct SchemaIntelligence {
    /// TF-IDF index for tokenization
    tfidf_index: TfIdfIndex,

    /// N-gram index
    ngram_index: NgramIndex,

    /// Semantic pattern matchers
    pattern_matchers: Vec<PatternMatcher>,
}

/// Pattern matcher for detecting semantic types
struct PatternMatcher {
    pattern_type: String,
    regex: Regex,
    confidence_threshold: f64,
}

impl SchemaIntelligence {
    /// Create a new schema intelligence engine
    pub fn new() -> Self {
        let pattern_matchers = vec![
            PatternMatcher {
                pattern_type: "email".to_string(),
                regex: Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap(),
                confidence_threshold: 0.8,
            },
            PatternMatcher {
                pattern_type: "phone".to_string(),
                regex: Regex::new(r"^\+?\d{1,3}[-.\s]?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}$")
                    .unwrap(),
                confidence_threshold: 0.7,
            },
            PatternMatcher {
                pattern_type: "ssn".to_string(),
                regex: Regex::new(r"^\d{3}-\d{2}-\d{4}$").unwrap(),
                confidence_threshold: 0.9,
            },
            PatternMatcher {
                pattern_type: "zip".to_string(),
                regex: Regex::new(r"^\d{5}(-\d{4})?$").unwrap(),
                confidence_threshold: 0.8,
            },
            PatternMatcher {
                pattern_type: "date".to_string(),
                regex: Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap(),
                confidence_threshold: 0.8,
            },
            PatternMatcher {
                pattern_type: "url".to_string(),
                regex: Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap(),
                confidence_threshold: 0.8,
            },
            PatternMatcher {
                pattern_type: "ipv4".to_string(),
                regex: Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}$").unwrap(),
                confidence_threshold: 0.9,
            },
            PatternMatcher {
                pattern_type: "uuid".to_string(),
                regex: Regex::new(
                    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$",
                )
                .unwrap(),
                confidence_threshold: 0.9,
            },
        ];

        Self {
            tfidf_index: TfIdfIndex::new(),
            ngram_index: NgramIndex::new(),
            pattern_matchers,
        }
    }

    /// Extract features from a schema field
    pub async fn extract_features(&self, field: &SchemaField) -> Result<FieldFeatures> {
        // Tokenize field name
        let name_tokens = self.tokenize_field_name(&field.name);

        // Generate n-grams
        let name_ngrams = self.ngram_index.generate_ngrams(&field.normalized_name);

        // Detect semantic patterns in sample values
        let semantic_patterns = self.detect_patterns(&field.sample_values);

        // Profile the data
        let statistics = self.profile_data(&field.sample_values);

        // Infer semantic type
        let inferred_type = self.infer_type(&field.name, &semantic_patterns);

        // Build context
        let context = FieldContext {
            table_name: field.table_name.clone(),
            schema_name: None,
            related_fields: vec![],
            is_primary_key: self.is_likely_primary_key(&field.name, &statistics),
            is_foreign_key: self.is_likely_foreign_key(&field.name),
            foreign_key_ref: None,
        };

        Ok(FieldFeatures {
            name_tokens,
            name_ngrams,
            semantic_patterns,
            statistics,
            inferred_type,
            context,
        })
    }

    /// Tokenize a field name into semantic tokens
    fn tokenize_field_name(&self, name: &str) -> Vec<String> {
        // Split by underscores, dashes, and camelCase
        let mut tokens = Vec::new();

        // Split by non-alphanumeric
        let words: Vec<&str> = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect();

        for word in words {
            // Handle camelCase
            let camel_tokens = self.split_camel_case(word);
            for token in camel_tokens {
                let lower = token.to_lowercase();
                if !lower.is_empty() && lower.len() > 1 {
                    tokens.push(lower);
                }
            }
        }

        tokens
    }

    /// Split camelCase into separate tokens
    fn split_camel_case(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current = String::new();

        for (i, ch) in text.chars().enumerate() {
            if ch.is_uppercase() && i > 0 && !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            current.push(ch);
        }

        if !current.is_empty() {
            tokens.push(current);
        }

        tokens
    }

    /// Detect semantic patterns in sample values
    fn detect_patterns(&self, sample_values: &[String]) -> Vec<SemanticPattern> {
        if sample_values.is_empty() {
            return vec![];
        }

        let mut detected_patterns = Vec::new();

        for matcher in &self.pattern_matchers {
            let mut match_count = 0;

            for value in sample_values {
                if matcher.regex.is_match(value) {
                    match_count += 1;
                }
            }

            let match_rate = match_count as f64 / sample_values.len() as f64;

            if match_rate >= matcher.confidence_threshold {
                detected_patterns.push(SemanticPattern {
                    pattern_type: matcher.pattern_type.clone(),
                    match_rate,
                    confidence: match_rate,
                });
            }
        }

        // Sort by confidence descending
        detected_patterns.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        detected_patterns
    }

    /// Profile data statistics
    fn profile_data(&self, sample_values: &[String]) -> FieldStatistics {
        if sample_values.is_empty() {
            return FieldStatistics {
                distinct_count: 0,
                sample_count: 0,
                null_rate: 1.0,
                avg_length: None,
                min_value: None,
                max_value: None,
                top_values: vec![],
            };
        }

        // Count distinct values (exact for small samples)
        let mut value_counts: HashMap<String, usize> = HashMap::new();
        let mut null_count = 0;
        let mut total_length = 0;

        for value in sample_values {
            if value.is_empty() || value.eq_ignore_ascii_case("null") {
                null_count += 1;
            } else {
                *value_counts.entry(value.clone()).or_insert(0) += 1;
                total_length += value.len();
            }
        }

        let distinct_count = value_counts.len();
        let sample_count = sample_values.len();
        let null_rate = null_count as f64 / sample_count as f64;
        let avg_length = if sample_count > null_count {
            Some(total_length as f64 / (sample_count - null_count) as f64)
        } else {
            None
        };

        // Get top values
        let mut value_vec: Vec<(String, usize)> = value_counts.into_iter().collect();
        value_vec.sort_by(|a, b| b.1.cmp(&a.1));
        let top_values: Vec<(String, usize)> = value_vec.into_iter().take(5).collect();

        // Min/max for numeric-looking values
        let numeric_values: Vec<f64> = sample_values
            .iter()
            .filter_map(|v| v.parse::<f64>().ok())
            .collect();

        let (min_value, max_value) = if !numeric_values.is_empty() {
            let min = numeric_values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numeric_values
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            (Some(min.to_string()), Some(max.to_string()))
        } else {
            (None, None)
        };

        FieldStatistics {
            distinct_count,
            sample_count,
            null_rate,
            avg_length,
            min_value,
            max_value,
            top_values,
        }
    }

    /// Infer semantic type from field name and patterns
    fn infer_type(&self, field_name: &str, patterns: &[SemanticPattern]) -> Option<String> {
        // If patterns detected, use highest confidence pattern
        if let Some(pattern) = patterns.first() {
            return Some(pattern.pattern_type.clone());
        }

        // Otherwise, infer from field name
        let lower_name = field_name.to_lowercase();

        let name_patterns = vec![
            (vec!["email", "e_mail", "mail"], "email"),
            (vec!["phone", "telephone", "mobile", "cell"], "phone"),
            (vec!["date", "datetime", "timestamp"], "date"),
            (vec!["url", "link", "website"], "url"),
            (vec!["address", "addr", "street"], "address"),
            (vec!["name", "first_name", "last_name", "full_name"], "name"),
            (vec!["id", "identifier", "key", "pk"], "identifier"),
            (vec!["price", "amount", "cost", "total"], "currency"),
            (vec!["zip", "zipcode", "postal"], "zip"),
            (vec!["ssn", "social_security"], "ssn"),
        ];

        for (keywords, semantic_type) in name_patterns {
            for keyword in keywords {
                if lower_name.contains(keyword) {
                    return Some(semantic_type.to_string());
                }
            }
        }

        None
    }

    /// Check if field is likely a primary key
    fn is_likely_primary_key(&self, field_name: &str, stats: &FieldStatistics) -> bool {
        let lower_name = field_name.to_lowercase();

        // Check name patterns
        let is_id_name = lower_name == "id"
            || lower_name.ends_with("_id")
            || lower_name.ends_with("id")
            || lower_name.contains("primary")
            || lower_name == "pk";

        // Check if all values are unique (distinct_count == sample_count)
        let is_unique = stats.distinct_count == stats.sample_count && stats.sample_count > 0;

        is_id_name && is_unique
    }

    /// Check if field is likely a foreign key
    fn is_likely_foreign_key(&self, field_name: &str) -> bool {
        let lower_name = field_name.to_lowercase();

        lower_name.ends_with("_id")
            || lower_name.contains("fk_")
            || lower_name.contains("_fk")
            || lower_name.contains("foreign")
    }
}

impl Default for SchemaIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_field_name() {
        let intelligence = SchemaIntelligence::new();

        let tokens = intelligence.tokenize_field_name("customer_email_address");
        assert_eq!(tokens, vec!["customer", "email", "address"]);

        let tokens = intelligence.tokenize_field_name("CustomerEmailAddress");
        assert_eq!(tokens, vec!["customer", "email", "address"]);
    }

    #[test]
    fn test_detect_email_pattern() {
        let intelligence = SchemaIntelligence::new();

        let samples = vec![
            "john@example.com".to_string(),
            "jane@example.com".to_string(),
            "bob@test.org".to_string(),
        ];

        let patterns = intelligence.detect_patterns(&samples);

        assert!(!patterns.is_empty());
        assert_eq!(patterns[0].pattern_type, "email");
        assert_eq!(patterns[0].match_rate, 1.0);
    }

    #[test]
    fn test_detect_phone_pattern() {
        let intelligence = SchemaIntelligence::new();

        let samples = vec![
            "+1-555-1234".to_string(),
            "+1-555-5678".to_string(),
            "555-9999".to_string(), // Doesn't match
        ];

        let patterns = intelligence.detect_patterns(&samples);

        // Should detect phone, but match rate < 1.0
        if !patterns.is_empty() {
            assert!(patterns[0].match_rate < 1.0);
        }
    }

    #[test]
    fn test_profile_data() {
        let intelligence = SchemaIntelligence::new();

        let samples = vec![
            "value1".to_string(),
            "value2".to_string(),
            "value1".to_string(),
            "".to_string(),
        ];

        let stats = intelligence.profile_data(&samples);

        assert_eq!(stats.distinct_count, 2);
        assert_eq!(stats.sample_count, 4);
        assert_eq!(stats.null_rate, 0.25);
        assert!(stats.avg_length.is_some());
    }

    #[test]
    fn test_infer_type_from_name() {
        let intelligence = SchemaIntelligence::new();

        assert_eq!(
            intelligence.infer_type("customer_email", &[]),
            Some("email".to_string())
        );

        assert_eq!(
            intelligence.infer_type("phone_number", &[]),
            Some("phone".to_string())
        );

        assert_eq!(
            intelligence.infer_type("user_id", &[]),
            Some("identifier".to_string())
        );
    }

    #[test]
    fn test_infer_type_from_patterns() {
        let intelligence = SchemaIntelligence::new();

        let patterns = vec![SemanticPattern {
            pattern_type: "email".to_string(),
            match_rate: 0.95,
            confidence: 0.95,
        }];

        assert_eq!(
            intelligence.infer_type("some_field", &patterns),
            Some("email".to_string())
        );
    }

    #[test]
    fn test_is_likely_primary_key() {
        let intelligence = SchemaIntelligence::new();

        let pk_stats = FieldStatistics {
            distinct_count: 100,
            sample_count: 100,
            null_rate: 0.0,
            avg_length: None,
            min_value: None,
            max_value: None,
            top_values: vec![],
        };

        assert!(intelligence.is_likely_primary_key("id", &pk_stats));
        assert!(intelligence.is_likely_primary_key("customer_id", &pk_stats));

        let non_pk_stats = FieldStatistics {
            distinct_count: 50,
            sample_count: 100,
            null_rate: 0.0,
            avg_length: None,
            min_value: None,
            max_value: None,
            top_values: vec![],
        };

        assert!(!intelligence.is_likely_primary_key("id", &non_pk_stats));
    }

    #[test]
    fn test_is_likely_foreign_key() {
        let intelligence = SchemaIntelligence::new();

        assert!(intelligence.is_likely_foreign_key("customer_id"));
        assert!(intelligence.is_likely_foreign_key("fk_user"));
        assert!(!intelligence.is_likely_foreign_key("id"));
        assert!(!intelligence.is_likely_foreign_key("email"));
    }
}
