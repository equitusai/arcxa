// graphica-core/src/inference/detectors.rs
//! PII/PHI detection and data classification utilities.

use crate::inference::types::*;
use regex::Regex;
use std::sync::OnceLock;

/// PII detector using pattern matching and heuristics
pub struct PiiDetector {
    email_pattern: Regex,
    phone_pattern: Regex,
    ssn_pattern: Regex,
    credit_card_pattern: Regex,
    ip_pattern: Regex,
}

impl PiiDetector {
    pub fn new() -> Self {
        Self {
            email_pattern: Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").unwrap(),
            phone_pattern: Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
            ssn_pattern: Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
            credit_card_pattern: Regex::new(r"\b\d{4}[- ]?\d{4}[- ]?\d{4}[- ]?\d{4}\b").unwrap(),
            ip_pattern: Regex::new(r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b").unwrap(),
        }
    }

    /// Detect PII based on column name
    pub fn detect_by_column_name(&self, column_name: &str) -> Option<(PiiType, f64)> {
        let lower = column_name.to_lowercase();

        if lower.contains("email") || lower.contains("e_mail") {
            return Some((PiiType::Email, 0.9));
        }
        if lower.contains("phone") || lower.contains("mobile") || lower.contains("tel") {
            return Some((PiiType::Phone, 0.85));
        }
        if lower.contains("ssn") || lower == "social_security_number" {
            return Some((PiiType::SSN, 0.95));
        }
        if lower.contains("credit_card") || lower.contains("card_number") {
            return Some((PiiType::CreditCard, 0.9));
        }
        if lower.contains("first_name")
            || lower.contains("last_name")
            || lower.contains("full_name")
        {
            return Some((PiiType::PersonName, 0.8));
        }
        if lower.contains("address")
            || lower.contains("street")
            || lower.contains("city")
            || lower.contains("zip")
        {
            return Some((PiiType::Address, 0.75));
        }
        if lower.contains("dob") || lower.contains("birth_date") || lower.contains("birthdate") {
            return Some((PiiType::DateOfBirth, 0.85));
        }
        if lower.contains("ip_address") || lower == "ip" {
            return Some((PiiType::IPAddress, 0.9));
        }

        None
    }

    /// Detect PII based on sample values
    pub fn detect_by_values(&self, samples: &[String]) -> Option<(PiiType, f64)> {
        let mut email_matches = 0;
        let mut phone_matches = 0;
        let mut ssn_matches = 0;
        let mut cc_matches = 0;
        let mut ip_matches = 0;

        for sample in samples.iter().take(100) {
            if self.email_pattern.is_match(sample) {
                email_matches += 1;
            }
            if self.phone_pattern.is_match(sample) {
                phone_matches += 1;
            }
            if self.ssn_pattern.is_match(sample) {
                ssn_matches += 1;
            }
            if self.credit_card_pattern.is_match(sample) && Self::luhn_check(sample) {
                cc_matches += 1;
            }
            if self.ip_pattern.is_match(sample) {
                ip_matches += 1;
            }
        }

        let total = samples.len().min(100);
        let threshold = 0.7; // 70% match rate

        if ssn_matches as f64 / total as f64 > threshold {
            return Some((PiiType::SSN, 0.95));
        }
        if cc_matches as f64 / total as f64 > threshold {
            return Some((PiiType::CreditCard, 0.9));
        }
        if email_matches as f64 / total as f64 > threshold {
            return Some((PiiType::Email, 0.9));
        }
        if phone_matches as f64 / total as f64 > threshold {
            return Some((PiiType::Phone, 0.85));
        }
        if ip_matches as f64 / total as f64 > threshold {
            return Some((PiiType::IPAddress, 0.8));
        }

        None
    }

    /// Luhn algorithm for credit card validation
    fn luhn_check(card: &str) -> bool {
        let digits: Vec<u32> = card
            .chars()
            .filter(|c| c.is_ascii_digit())
            .map(|c| c.to_digit(10).unwrap())
            .collect();

        if digits.len() < 13 || digits.len() > 19 {
            return false;
        }

        let sum: u32 = digits
            .iter()
            .rev()
            .enumerate()
            .map(|(i, &d)| {
                if i % 2 == 1 {
                    let doubled = d * 2;
                    if doubled > 9 {
                        doubled - 9
                    } else {
                        doubled
                    }
                } else {
                    d
                }
            })
            .sum();

        sum % 10 == 0
    }

    /// Combined detection (name + values)
    pub fn detect_pii(&self, column_name: &str, samples: &[String]) -> Option<PiiDetection> {
        let name_detection = self.detect_by_column_name(column_name);
        let value_detection = self.detect_by_values(samples);

        match (name_detection, value_detection) {
            (Some((name_type, name_conf)), Some((val_type, val_conf))) => {
                if name_type == val_type {
                    // Both agree - high confidence
                    Some(PiiDetection {
                        pii_type: name_type,
                        confidence: (name_conf + val_conf) / 2.0,
                        detection_method: DetectionMethod::ValuePattern,
                        sample_matches: Self::redact_samples(samples, 3),
                    })
                } else {
                    // Name suggests one thing, values another - trust values more
                    Some(PiiDetection {
                        pii_type: val_type,
                        confidence: val_conf * 0.8, // Slightly lower due to mismatch
                        detection_method: DetectionMethod::ValuePattern,
                        sample_matches: Self::redact_samples(samples, 3),
                    })
                }
            }
            (Some((pii_type, conf)), None) => Some(PiiDetection {
                pii_type,
                confidence: conf * 0.7, // Lower confidence without value confirmation
                detection_method: DetectionMethod::ColumnName,
                sample_matches: Self::redact_samples(samples, 3),
            }),
            (None, Some((pii_type, conf))) => Some(PiiDetection {
                pii_type,
                confidence: conf,
                detection_method: DetectionMethod::ValuePattern,
                sample_matches: Self::redact_samples(samples, 3),
            }),
            (None, None) => None,
        }
    }

    fn redact_samples(samples: &[String], count: usize) -> Vec<String> {
        samples
            .iter()
            .take(count)
            .map(|s| {
                if s.len() > 4 {
                    format!("{}***", &s[..2])
                } else {
                    "***".to_string()
                }
            })
            .collect()
    }
}

impl Default for PiiDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// Data quality calculator
pub struct QualityCalculator;

impl QualityCalculator {
    /// Calculate completeness (% non-null)
    pub fn completeness(null_count: u64, total_count: u64) -> f64 {
        if total_count == 0 {
            return 1.0;
        }
        ((total_count - null_count) as f64 / total_count as f64) * 100.0
    }

    /// Calculate uniqueness (% unique values)
    pub fn uniqueness(distinct_count: u64, total_count: u64) -> f64 {
        if total_count == 0 {
            return 1.0;
        }
        (distinct_count as f64 / total_count as f64) * 100.0
    }

    /// Calculate validity based on format violations
    pub fn validity(valid_count: u64, total_count: u64) -> f64 {
        if total_count == 0 {
            return 1.0;
        }
        (valid_count as f64 / total_count as f64) * 100.0
    }

    /// Calculate timeliness based on last modified time
    pub fn timeliness(last_modified: Option<chrono::DateTime<chrono::Utc>>) -> f64 {
        use chrono::Utc;

        if let Some(modified) = last_modified {
            let age_hours = (Utc::now() - modified).num_hours();

            // Scoring: 100% if < 1 hour, linear decay to 0% at 7 days
            if age_hours < 1 {
                100.0
            } else if age_hours > 168 {
                // 7 days
                0.0
            } else {
                100.0 - (age_hours as f64 / 168.0 * 100.0)
            }
        } else {
            50.0 // Unknown - assume moderate
        }
    }

    /// Calculate overall quality score
    pub fn overall_score(metrics: &DataQualityMetrics) -> f64 {
        let weights = [0.3, 0.2, 0.2, 0.15, 0.15]; // completeness, uniqueness, validity, consistency, timeliness

        weights[0] * metrics.completeness
            + weights[1] * metrics.uniqueness
            + weights[2] * metrics.validity
            + weights[3] * metrics.consistency
            + weights[4] * metrics.timeliness
    }
}

/// Data classifier based on content and patterns
pub struct DataClassifier;

impl DataClassifier {
    /// Classify data based on PII detection and patterns
    pub fn classify(
        table_name: &str,
        pii_detections: &[(String, PiiDetection)],
    ) -> DataClassification {
        // Check for highly restricted data
        for (_col, detection) in pii_detections {
            if matches!(
                detection.pii_type,
                PiiType::SSN | PiiType::CreditCard | PiiType::MedicalRecordNumber
            ) && detection.confidence > 0.8
            {
                return DataClassification::HighlyRestricted;
            }
        }

        // Check for restricted data
        if !pii_detections.is_empty() {
            let high_conf_pii = pii_detections
                .iter()
                .filter(|(_, d)| d.confidence > 0.7)
                .count();
            if high_conf_pii > 0 {
                return DataClassification::Restricted;
            }
        }

        // Check table name patterns
        let lower = table_name.to_lowercase();
        if lower.contains("customer")
            || lower.contains("employee")
            || lower.contains("user")
            || lower.contains("account")
        {
            return DataClassification::Confidential;
        }

        if lower.contains("public") || lower.contains("product") || lower.contains("catalog") {
            return DataClassification::Public;
        }

        // Default
        DataClassification::Internal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_detection() {
        let detector = PiiDetector::new();
        let samples = vec![
            "john@example.com".to_string(),
            "jane.doe@company.org".to_string(),
            "test@test.com".to_string(),
        ];

        let result = detector.detect_by_values(&samples);
        assert!(result.is_some());
        let (pii_type, conf) = result.unwrap();
        assert!(matches!(pii_type, PiiType::Email));
        assert!(conf > 0.8);
    }

    #[test]
    fn test_column_name_detection() {
        let detector = PiiDetector::new();
        assert!(detector.detect_by_column_name("user_email").is_some());
        assert!(detector.detect_by_column_name("phone_number").is_some());
        assert!(detector.detect_by_column_name("product_name").is_none());
    }

    #[test]
    fn test_luhn_check() {
        assert!(PiiDetector::luhn_check("4532015112830366")); // Valid Visa
        assert!(!PiiDetector::luhn_check("1234567890123456")); // Invalid
    }

    #[test]
    fn test_quality_completeness() {
        assert_eq!(QualityCalculator::completeness(10, 100), 90.0);
        assert_eq!(QualityCalculator::completeness(0, 100), 100.0);
    }
}
