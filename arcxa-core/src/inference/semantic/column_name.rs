//! Column name-based semantic type detection
//!
//! This strategy detects semantic types by analyzing column names using:
//! - Exact matches (case-insensitive)
//! - Partial matches (contains, starts_with, ends_with)
//! - Pattern matching (regex)
//! - Common naming conventions (snake_case, camelCase, etc.)

use anyhow::Result;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

use super::strategy::DetectionStrategy;
use super::types::{DetectionContext, DetectionEvidence, DetectionResult, EvidenceType};
use crate::inference::types::SemanticType;

/// Column name patterns for each semantic type
static NAME_PATTERNS: Lazy<HashMap<SemanticType, Vec<ColumnNamePattern>>> = Lazy::new(|| {
    let mut patterns = HashMap::new();

    // Email
    patterns.insert(
        SemanticType::Email,
        vec![
            ColumnNamePattern::exact("email"),
            ColumnNamePattern::exact("email_address"),
            ColumnNamePattern::exact("e_mail"),
            ColumnNamePattern::ends_with("_email"),
            ColumnNamePattern::ends_with("_mail"),
        ],
    );

    // Phone Number
    patterns.insert(
        SemanticType::PhoneNumber,
        vec![
            ColumnNamePattern::exact("phone"),
            ColumnNamePattern::exact("phone_number"),
            ColumnNamePattern::exact("telephone"),
            ColumnNamePattern::exact("mobile"),
            ColumnNamePattern::exact("cell"),
            ColumnNamePattern::contains("phone"),
            ColumnNamePattern::contains("tel"),
        ],
    );

    // Person Name
    patterns.insert(
        SemanticType::PersonName,
        vec![
            ColumnNamePattern::exact("name"),
            ColumnNamePattern::exact("full_name"),
            ColumnNamePattern::exact("fullname"),
            ColumnNamePattern::exact("person_name"),
            ColumnNamePattern::exact("customer_name"),
            ColumnNamePattern::exact("user_name"),
            ColumnNamePattern::exact("username"),
        ],
    );

    // Address
    patterns.insert(
        SemanticType::Address,
        vec![
            ColumnNamePattern::exact("address"),
            ColumnNamePattern::exact("street_address"),
            ColumnNamePattern::exact("mailing_address"),
            ColumnNamePattern::exact("billing_address"),
            ColumnNamePattern::exact("shipping_address"),
            ColumnNamePattern::ends_with("_address"),
        ],
    );

    // City
    patterns.insert(
        SemanticType::City,
        vec![
            ColumnNamePattern::exact("city"),
            ColumnNamePattern::exact("town"),
            ColumnNamePattern::ends_with("_city"),
        ],
    );

    // State
    patterns.insert(
        SemanticType::State,
        vec![
            ColumnNamePattern::exact("state"),
            ColumnNamePattern::exact("province"),
            ColumnNamePattern::exact("region"),
            ColumnNamePattern::ends_with("_state"),
        ],
    );

    // Postal Code
    patterns.insert(
        SemanticType::PostalCode,
        vec![
            ColumnNamePattern::exact("zip"),
            ColumnNamePattern::exact("zipcode"),
            ColumnNamePattern::exact("zip_code"),
            ColumnNamePattern::exact("postal_code"),
            ColumnNamePattern::exact("postcode"),
        ],
    );

    // Country
    patterns.insert(
        SemanticType::Country,
        vec![
            ColumnNamePattern::exact("country"),
            ColumnNamePattern::exact("country_name"),
            ColumnNamePattern::ends_with("_country"),
        ],
    );

    // Country Code
    patterns.insert(
        SemanticType::CountryCode,
        vec![
            ColumnNamePattern::exact("country_code"),
            ColumnNamePattern::exact("country_cd"),
            ColumnNamePattern::exact("iso_country"),
        ],
    );

    // SSN
    patterns.insert(
        SemanticType::SSN,
        vec![
            ColumnNamePattern::exact("ssn"),
            ColumnNamePattern::exact("social_security_number"),
            ColumnNamePattern::exact("social_security"),
        ],
    );

    // Credit Card
    patterns.insert(
        SemanticType::CreditCardNumber,
        vec![
            ColumnNamePattern::exact("credit_card"),
            ColumnNamePattern::exact("cc_number"),
            ColumnNamePattern::exact("card_number"),
            ColumnNamePattern::exact("creditcard"),
            ColumnNamePattern::contains("credit_card"),
        ],
    );

    // Currency Amount
    patterns.insert(
        SemanticType::CurrencyAmount,
        vec![
            ColumnNamePattern::exact("amount"),
            ColumnNamePattern::exact("price"),
            ColumnNamePattern::exact("cost"),
            ColumnNamePattern::exact("total"),
            ColumnNamePattern::exact("balance"),
            ColumnNamePattern::exact("fee"),
            ColumnNamePattern::ends_with("_amount"),
            ColumnNamePattern::ends_with("_price"),
            ColumnNamePattern::ends_with("_cost"),
        ],
    );

    // Timestamp
    patterns.insert(
        SemanticType::Timestamp,
        vec![
            ColumnNamePattern::exact("timestamp"),
            ColumnNamePattern::exact("created_at"),
            ColumnNamePattern::exact("updated_at"),
            ColumnNamePattern::exact("modified_at"),
            ColumnNamePattern::exact("deleted_at"),
            ColumnNamePattern::ends_with("_timestamp"),
            ColumnNamePattern::ends_with("_at"),
        ],
    );

    // Date
    patterns.insert(
        SemanticType::Date,
        vec![
            ColumnNamePattern::exact("date"),
            ColumnNamePattern::exact("birth_date"),
            ColumnNamePattern::exact("start_date"),
            ColumnNamePattern::exact("end_date"),
            ColumnNamePattern::ends_with("_date"),
            ColumnNamePattern::ends_with("_dt"),
        ],
    );

    // Date of Birth
    patterns.insert(
        SemanticType::DateOfBirth,
        vec![
            ColumnNamePattern::exact("dob"),
            ColumnNamePattern::exact("date_of_birth"),
            ColumnNamePattern::exact("birth_date"),
            ColumnNamePattern::exact("birthdate"),
        ],
    );

    // URL
    patterns.insert(
        SemanticType::URL,
        vec![
            ColumnNamePattern::exact("url"),
            ColumnNamePattern::exact("website"),
            ColumnNamePattern::exact("link"),
            ColumnNamePattern::ends_with("_url"),
            ColumnNamePattern::ends_with("_link"),
        ],
    );

    // UUID
    patterns.insert(
        SemanticType::UUID,
        vec![
            ColumnNamePattern::exact("uuid"),
            ColumnNamePattern::exact("guid"),
            ColumnNamePattern::ends_with("_uuid"),
            ColumnNamePattern::ends_with("_guid"),
        ],
    );

    // IP Address
    patterns.insert(
        SemanticType::IPAddress,
        vec![
            ColumnNamePattern::exact("ip"),
            ColumnNamePattern::exact("ip_address"),
            ColumnNamePattern::exact("ipaddress"),
            ColumnNamePattern::ends_with("_ip"),
        ],
    );

    // User ID
    patterns.insert(
        SemanticType::UserId,
        vec![
            ColumnNamePattern::exact("user_id"),
            ColumnNamePattern::exact("userid"),
            ColumnNamePattern::exact("uid"),
        ],
    );

    // Product Code
    patterns.insert(
        SemanticType::ProductCode,
        vec![
            ColumnNamePattern::exact("product_code"),
            ColumnNamePattern::exact("product_id"),
            ColumnNamePattern::exact("sku"),
        ],
    );

    // SKU
    patterns.insert(
        SemanticType::SKU,
        vec![
            ColumnNamePattern::exact("sku"),
            ColumnNamePattern::exact("product_sku"),
        ],
    );

    // Status
    patterns.insert(
        SemanticType::Status,
        vec![
            ColumnNamePattern::exact("status"),
            ColumnNamePattern::exact("state"),
            ColumnNamePattern::ends_with("_status"),
            ColumnNamePattern::ends_with("_state"),
        ],
    );

    // Description
    patterns.insert(
        SemanticType::Description,
        vec![
            ColumnNamePattern::exact("description"),
            ColumnNamePattern::exact("desc"),
            ColumnNamePattern::exact("notes"),
            ColumnNamePattern::exact("comments"),
            ColumnNamePattern::ends_with("_description"),
            ColumnNamePattern::ends_with("_desc"),
        ],
    );

    patterns
});

/// Column name pattern matcher
#[derive(Debug, Clone)]
enum ColumnNamePattern {
    Exact(String),
    StartsWith(String),
    EndsWith(String),
    Contains(String),
    Regex(String),
}

impl ColumnNamePattern {
    fn exact(s: &str) -> Self {
        Self::Exact(s.to_lowercase())
    }

    fn starts_with(s: &str) -> Self {
        Self::StartsWith(s.to_lowercase())
    }

    fn ends_with(s: &str) -> Self {
        Self::EndsWith(s.to_lowercase())
    }

    fn contains(s: &str) -> Self {
        Self::Contains(s.to_lowercase())
    }

    fn regex(pattern: &str) -> Self {
        Self::Regex(pattern.to_string())
    }

    /// Check if the column name matches this pattern
    fn matches(&self, column_name: &str) -> bool {
        let normalized = Self::normalize_column_name(column_name);

        match self {
            Self::Exact(pattern) => normalized == *pattern,
            Self::StartsWith(pattern) => normalized.starts_with(pattern),
            Self::EndsWith(pattern) => normalized.ends_with(pattern),
            Self::Contains(pattern) => normalized.contains(pattern),
            Self::Regex(pattern) => Regex::new(pattern)
                .map(|re| re.is_match(&normalized))
                .unwrap_or(false),
        }
    }

    /// Normalize column name for matching
    ///
    /// - Convert to lowercase
    /// - Replace common separators with underscore
    /// - Remove special characters
    fn normalize_column_name(name: &str) -> String {
        name.to_lowercase()
            .replace('-', "_")
            .replace(' ', "_")
            .replace('.', "_")
    }
}

/// Column name-based detection strategy
pub struct ColumnNameDetector {
    /// Confidence boost for exact matches
    exact_match_confidence: f64,

    /// Confidence for partial matches
    partial_match_confidence: f64,
}

impl Default for ColumnNameDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl ColumnNameDetector {
    /// Create a new column name detector
    pub fn new() -> Self {
        Self {
            exact_match_confidence: 0.9,
            partial_match_confidence: 0.7,
        }
    }

    /// Detect semantic type from column name (synchronous, for dataflow operators)
    ///
    /// Returns (SemanticType, confidence, description) if a match is found.
    pub fn detect_from_name(&self, column_name: &str) -> Option<(SemanticType, f64, String)> {
        for (semantic_type, patterns) in NAME_PATTERNS.iter() {
            for pattern in patterns {
                if pattern.matches(column_name) {
                    let confidence = match pattern {
                        ColumnNamePattern::Exact(_) => self.exact_match_confidence,
                        _ => self.partial_match_confidence,
                    };

                    let description = match pattern {
                        ColumnNamePattern::Exact(p) => format!("Exact match: '{}'", p),
                        ColumnNamePattern::StartsWith(p) => format!("Starts with: '{}'", p),
                        ColumnNamePattern::EndsWith(p) => format!("Ends with: '{}'", p),
                        ColumnNamePattern::Contains(p) => format!("Contains: '{}'", p),
                        ColumnNamePattern::Regex(p) => format!("Regex match: '{}'", p),
                    };

                    return Some((semantic_type.clone(), confidence, description));
                }
            }
        }

        None
    }
}

#[async_trait]
impl DetectionStrategy for ColumnNameDetector {
    fn name(&self) -> &str {
        "column_name"
    }

    fn priority(&self) -> f64 {
        0.8 // High priority - column names are reliable
    }

    async fn detect(&self, context: &DetectionContext) -> Result<Option<DetectionResult>> {
        if let Some((semantic_type, confidence, description)) =
            self.detect_from_name(&context.column_name)
        {
            let evidence =
                DetectionEvidence::new(EvidenceType::ColumnName, description, confidence);

            let result = DetectionResult::new(semantic_type, confidence, self.name())
                .with_evidence(evidence);

            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    fn is_applicable(&self, _context: &DetectionContext) -> bool {
        true // Always applicable
    }

    fn min_sample_size(&self) -> usize {
        0 // Doesn't need samples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_column_name() {
        assert_eq!(
            ColumnNamePattern::normalize_column_name("Email-Address"),
            "email_address"
        );
        assert_eq!(
            ColumnNamePattern::normalize_column_name("User.Name"),
            "user_name"
        );
        assert_eq!(
            ColumnNamePattern::normalize_column_name("PHONE NUMBER"),
            "phone_number"
        );
    }

    #[test]
    fn test_pattern_matching() {
        let exact = ColumnNamePattern::exact("email");
        assert!(exact.matches("email"));
        assert!(exact.matches("EMAIL"));
        assert!(exact.matches("Email"));
        assert!(!exact.matches("user_email"));

        let ends_with = ColumnNamePattern::ends_with("_email");
        assert!(ends_with.matches("user_email"));
        assert!(ends_with.matches("customer_email"));
        assert!(!ends_with.matches("email"));

        let contains = ColumnNamePattern::contains("phone");
        assert!(contains.matches("phone"));
        assert!(contains.matches("phone_number"));
        assert!(contains.matches("mobile_phone"));
    }

    #[tokio::test]
    async fn test_email_detection() {
        let detector = ColumnNameDetector::new();
        let context = DetectionContext::new("email", "varchar");

        let result = detector.detect(&context).await.unwrap();
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::Email);
        assert!(result.confidence >= 0.9);
        assert_eq!(result.evidence.len(), 1);
    }

    #[tokio::test]
    async fn test_phone_detection() {
        let detector = ColumnNameDetector::new();
        let context = DetectionContext::new("mobile_phone", "varchar");

        let result = detector.detect(&context).await.unwrap();
        assert!(result.is_some());

        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::PhoneNumber);
    }

    #[tokio::test]
    async fn test_no_match() {
        let detector = ColumnNameDetector::new();
        let context = DetectionContext::new("some_random_column", "integer");

        let result = detector.detect(&context).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_timestamp_detection() {
        let detector = ColumnNameDetector::new();

        let test_cases = vec!["created_at", "updated_at", "deleted_at", "timestamp"];

        for column_name in test_cases {
            let context = DetectionContext::new(column_name, "timestamp");
            let result = detector.detect(&context).await.unwrap();
            assert!(result.is_some(), "Failed for column: {}", column_name);
            assert_eq!(result.unwrap().semantic_type, SemanticType::Timestamp);
        }
    }

    #[tokio::test]
    async fn test_currency_detection() {
        let detector = ColumnNameDetector::new();

        let test_cases = vec!["amount", "price", "total_amount", "unit_price"];

        for column_name in test_cases {
            let context = DetectionContext::new(column_name, "decimal");
            let result = detector.detect(&context).await.unwrap();
            assert!(result.is_some(), "Failed for column: {}", column_name);
            assert_eq!(result.unwrap().semantic_type, SemanticType::CurrencyAmount);
        }
    }
}
