//! Semantic Type Detection
//!
//! Automatically detects semantic types from field names and sample data
//! using pattern matching, regex validation, and statistical analysis.
//!
//! Supports ontology alignment via the model service for custom domain types.

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;

use super::field::{SemanticType, SensitivityLevel};

/// Semantic type detection result
#[derive(Debug, Clone)]
pub struct SemanticDetectionResult {
    /// Detected semantic type
    pub semantic_type: SemanticType,

    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,

    /// Sensitivity level suggestion
    pub suggested_sensitivity: Option<SensitivityLevel>,

    /// Detection method used
    pub detection_method: DetectionMethod,
}

/// Method used for detection
#[derive(Debug, Clone, PartialEq)]
pub enum DetectionMethod {
    /// Field name pattern matching
    FieldNamePattern,

    /// Data format validation (regex)
    DataFormatValidation,

    /// Statistical analysis
    StatisticalAnalysis,

    /// Combination of methods
    Combined,

    /// ML-based ontology alignment (via model service)
    OntologyAlignment,
}

lazy_static! {
    /// Regex patterns for semantic type detection
    static ref EMAIL_REGEX: Regex = Regex::new(
        r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$"
    ).unwrap();

    static ref PHONE_REGEX: Regex = Regex::new(
        r"^(\+?\d{1,3}[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}$"
    ).unwrap();

    static ref SSN_REGEX: Regex = Regex::new(
        r"^\d{3}-\d{2}-\d{4}$"
    ).unwrap();

    static ref CREDIT_CARD_REGEX: Regex = Regex::new(
        r"^\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}$"
    ).unwrap();

    static ref UUID_REGEX: Regex = Regex::new(
        r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
    ).unwrap();

    static ref IP_ADDRESS_REGEX: Regex = Regex::new(
        r"^(\d{1,3}\.){3}\d{1,3}$"
    ).unwrap();

    static ref URL_REGEX: Regex = Regex::new(
        r"^https?://[^\s/$.?#].[^\s]*$"
    ).unwrap();

    static ref POSTAL_CODE_US_REGEX: Regex = Regex::new(
        r"^\d{5}(-\d{4})?$"
    ).unwrap();

    /// Field name patterns for semantic types
    static ref FIELD_NAME_PATTERNS: HashMap<SemanticType, Vec<&'static str>> = {
        let mut m = HashMap::new();

        m.insert(SemanticType::Email, vec!["email", "e_mail", "email_address", "mail"]);
        m.insert(SemanticType::PhoneNumber, vec!["phone", "phone_number", "telephone", "mobile", "cell"]);
        m.insert(SemanticType::SocialSecurityNumber, vec!["ssn", "social_security", "social_security_number"]);
        m.insert(SemanticType::CreditCardNumber, vec!["cc", "credit_card", "card_number", "cc_number"]);

        m.insert(SemanticType::FirstName, vec!["first_name", "firstname", "fname", "given_name"]);
        m.insert(SemanticType::LastName, vec!["last_name", "lastname", "lname", "surname", "family_name"]);
        m.insert(SemanticType::FullName, vec!["full_name", "fullname", "name", "customer_name"]);

        m.insert(SemanticType::StreetAddress, vec!["street", "address", "street_address", "addr"]);
        m.insert(SemanticType::City, vec!["city", "town"]);
        m.insert(SemanticType::State, vec!["state", "province", "region"]);
        m.insert(SemanticType::PostalCode, vec!["zip", "zipcode", "postal_code", "postcode"]);
        m.insert(SemanticType::Country, vec!["country", "country_code", "nation"]);

        m.insert(SemanticType::Latitude, vec!["lat", "latitude"]);
        m.insert(SemanticType::Longitude, vec!["lon", "lng", "longitude"]);

        m.insert(SemanticType::BirthDate, vec!["birth_date", "birthdate", "dob", "date_of_birth"]);
        m.insert(SemanticType::Age, vec!["age", "years_old"]);

        m.insert(SemanticType::UUID, vec!["uuid", "guid", "unique_id"]);
        m.insert(SemanticType::CustomerId, vec!["customer_id", "cust_id", "client_id"]);
        m.insert(SemanticType::OrderNumber, vec!["order_number", "order_id", "order_no"]);
        m.insert(SemanticType::InvoiceNumber, vec!["invoice_number", "invoice_id", "invoice_no"]);

        m.insert(SemanticType::Price, vec!["price", "cost", "rate", "unit_price"]);
        m.insert(SemanticType::Amount, vec!["amount", "total", "subtotal", "total_amount"]);
        m.insert(SemanticType::Currency, vec!["currency", "currency_code"]);
        m.insert(SemanticType::Percentage, vec!["percent", "percentage", "pct"]);

        m.insert(SemanticType::IPAddress, vec!["ip", "ip_address", "ip_addr", "ipaddr"]);
        m.insert(SemanticType::URL, vec!["url", "website", "link", "web_address"]);
        m.insert(SemanticType::Domain, vec!["domain", "hostname", "host"]);

        m.insert(SemanticType::Gender, vec!["gender", "sex"]);
        m.insert(SemanticType::CompanyName, vec!["company", "company_name", "organization", "org"]);
        m.insert(SemanticType::JobTitle, vec!["job_title", "title", "position"]);
        m.insert(SemanticType::Department, vec!["department", "dept", "division"]);

        m
    };
}

/// Semantic type detector
pub struct SemanticDetector {
    /// Minimum confidence threshold
    confidence_threshold: f64,

    /// Minimum match ratio for data validation (0.0 - 1.0)
    min_match_ratio: f64,
}

impl SemanticDetector {
    /// Create a new semantic detector
    pub fn new() -> Self {
        Self {
            confidence_threshold: 0.7,
            min_match_ratio: 0.8,
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(confidence_threshold: f64, min_match_ratio: f64) -> Self {
        Self {
            confidence_threshold,
            min_match_ratio,
        }
    }

    /// Detect semantic type from field name and sample data
    pub fn detect(
        &self,
        field_name: &str,
        sample_values: &[Option<String>],
    ) -> Option<SemanticDetectionResult> {
        // Try field name pattern matching first
        if let Some(result) = self.detect_from_field_name(field_name) {
            // Validate with data if available
            if !sample_values.is_empty() {
                if let Some(validation_result) =
                    self.validate_with_data(&result.semantic_type, sample_values)
                {
                    return Some(validation_result);
                }
            }
            return Some(result);
        }

        // Try data format validation
        if !sample_values.is_empty() {
            if let Some(result) = self.detect_from_data(sample_values) {
                return Some(result);
            }
        }

        None
    }

    /// Detect from field name patterns
    fn detect_from_field_name(&self, field_name: &str) -> Option<SemanticDetectionResult> {
        let normalized = field_name.to_lowercase().replace(['-', ' '], "_");

        // Collect all matches to find the best one (prioritize exact matches over partial)
        let mut best_match: Option<(SemanticType, f64)> = None;

        for (semantic_type, patterns) in FIELD_NAME_PATTERNS.iter() {
            for pattern in patterns {
                if normalized.contains(pattern) {
                    let confidence = if normalized == *pattern {
                        0.95 // Exact match
                    } else {
                        0.75 // Partial match
                    };

                    // Keep the best match (highest confidence, or exact match over partial)
                    match &best_match {
                        None => {
                            best_match = Some((semantic_type.clone(), confidence));
                        }
                        Some((_, prev_confidence)) => {
                            if confidence > *prev_confidence {
                                best_match = Some((semantic_type.clone(), confidence));
                            }
                        }
                    }
                }
            }
        }

        // Return the best match if found
        best_match.map(|(semantic_type, confidence)| {
            let sensitivity = Self::suggest_sensitivity(&semantic_type);
            SemanticDetectionResult {
                semantic_type,
                confidence,
                suggested_sensitivity: sensitivity,
                detection_method: DetectionMethod::FieldNamePattern,
            }
        })
    }

    /// Detect from data format patterns
    fn detect_from_data(
        &self,
        sample_values: &[Option<String>],
    ) -> Option<SemanticDetectionResult> {
        let non_null_values: Vec<&String> =
            sample_values.iter().filter_map(|v| v.as_ref()).collect();

        if non_null_values.is_empty() {
            return None;
        }

        // Test each pattern type
        let patterns = vec![
            (SemanticType::Email, &*EMAIL_REGEX),
            (SemanticType::PhoneNumber, &*PHONE_REGEX),
            (SemanticType::SocialSecurityNumber, &*SSN_REGEX),
            (SemanticType::CreditCardNumber, &*CREDIT_CARD_REGEX),
            (SemanticType::UUID, &*UUID_REGEX),
            (SemanticType::IPAddress, &*IP_ADDRESS_REGEX),
            (SemanticType::URL, &*URL_REGEX),
            (SemanticType::PostalCode, &*POSTAL_CODE_US_REGEX),
        ];

        for (semantic_type, regex) in patterns {
            let matches = non_null_values
                .iter()
                .filter(|v| regex.is_match(v.trim()))
                .count();

            let match_ratio = matches as f64 / non_null_values.len() as f64;

            if match_ratio >= self.min_match_ratio {
                let confidence = match_ratio * 0.95; // High confidence for regex validation
                let sensitivity = Self::suggest_sensitivity(&semantic_type);

                return Some(SemanticDetectionResult {
                    semantic_type,
                    confidence,
                    suggested_sensitivity: sensitivity,
                    detection_method: DetectionMethod::DataFormatValidation,
                });
            }
        }

        None
    }

    /// Validate field name detection with actual data
    fn validate_with_data(
        &self,
        semantic_type: &SemanticType,
        sample_values: &[Option<String>],
    ) -> Option<SemanticDetectionResult> {
        let non_null_values: Vec<&String> =
            sample_values.iter().filter_map(|v| v.as_ref()).collect();

        if non_null_values.is_empty() {
            return None;
        }

        let regex = match semantic_type {
            SemanticType::Email => &*EMAIL_REGEX,
            SemanticType::PhoneNumber => &*PHONE_REGEX,
            SemanticType::SocialSecurityNumber => &*SSN_REGEX,
            SemanticType::CreditCardNumber => &*CREDIT_CARD_REGEX,
            SemanticType::UUID => &*UUID_REGEX,
            SemanticType::IPAddress => &*IP_ADDRESS_REGEX,
            SemanticType::URL => &*URL_REGEX,
            SemanticType::PostalCode => &*POSTAL_CODE_US_REGEX,
            _ => return None, // Can't validate this type with regex
        };

        let matches = non_null_values
            .iter()
            .filter(|v| regex.is_match(v.trim()))
            .count();

        let match_ratio = matches as f64 / non_null_values.len() as f64;

        if match_ratio >= self.min_match_ratio {
            let confidence = (0.75 + match_ratio * 0.25).min(0.99); // Combined confidence
            let sensitivity = Self::suggest_sensitivity(semantic_type);

            Some(SemanticDetectionResult {
                semantic_type: semantic_type.clone(),
                confidence,
                suggested_sensitivity: sensitivity,
                detection_method: DetectionMethod::Combined,
            })
        } else {
            None
        }
    }

    /// Suggest sensitivity level based on semantic type
    fn suggest_sensitivity(semantic_type: &SemanticType) -> Option<SensitivityLevel> {
        match semantic_type {
            SemanticType::SocialSecurityNumber
            | SemanticType::CreditCardNumber
            | SemanticType::BankAccountNumber
            | SemanticType::PassportNumber
            | SemanticType::DriversLicense => Some(SensitivityLevel::Restricted),

            SemanticType::Email
            | SemanticType::PhoneNumber
            | SemanticType::FullName
            | SemanticType::FirstName
            | SemanticType::LastName
            | SemanticType::BirthDate
            | SemanticType::FullAddress
            | SemanticType::StreetAddress => Some(SensitivityLevel::Confidential),

            SemanticType::CompanyName
            | SemanticType::JobTitle
            | SemanticType::Department
            | SemanticType::City
            | SemanticType::State
            | SemanticType::Country => Some(SensitivityLevel::Internal),

            _ => Some(SensitivityLevel::Public),
        }
    }
}

impl Default for SemanticDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_detection_from_name() {
        let detector = SemanticDetector::new();
        let result = detector.detect("email_address", &[]);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::Email);
        assert!(result.confidence >= 0.7);
        assert_eq!(
            result.suggested_sensitivity,
            Some(SensitivityLevel::Confidential)
        );
    }

    #[test]
    fn test_email_detection_from_data() {
        let detector = SemanticDetector::new();
        let samples = vec![
            Some("john.doe@example.com".to_string()),
            Some("jane.smith@company.org".to_string()),
            Some("admin@test.com".to_string()),
        ];

        let result = detector.detect("user_contact", &samples);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::Email);
        assert_eq!(
            result.detection_method,
            DetectionMethod::DataFormatValidation
        );
    }

    #[test]
    fn test_phone_detection() {
        let detector = SemanticDetector::new();
        let samples = vec![
            Some("555-123-4567".to_string()),
            Some("(555) 987-6543".to_string()),
            Some("555.456.7890".to_string()),
        ];

        let result = detector.detect("phone_number", &samples);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::PhoneNumber);
        assert!(result.confidence >= 0.9);
        assert_eq!(result.detection_method, DetectionMethod::Combined);
    }

    #[test]
    fn test_ssn_detection() {
        let detector = SemanticDetector::new();
        let samples = vec![
            Some("123-45-6789".to_string()),
            Some("987-65-4321".to_string()),
        ];

        let result = detector.detect("ssn", &samples);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::SocialSecurityNumber);
        assert_eq!(
            result.suggested_sensitivity,
            Some(SensitivityLevel::Restricted)
        );
    }

    #[test]
    fn test_uuid_detection() {
        let detector = SemanticDetector::new();
        let samples = vec![
            Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
            Some("6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string()),
        ];

        let result = detector.detect("id", &samples);

        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.semantic_type, SemanticType::UUID);
    }

    #[test]
    fn test_no_detection() {
        let detector = SemanticDetector::new();
        let samples = vec![
            Some("random text".to_string()),
            Some("some data".to_string()),
        ];

        let result = detector.detect("unknown_field", &samples);
        assert!(result.is_none());
    }

    #[test]
    fn test_field_name_patterns() {
        let detector = SemanticDetector::new();

        assert!(detector.detect("first_name", &[]).is_some());
        assert!(detector.detect("last_name", &[]).is_some());
        assert!(detector.detect("customer_id", &[]).is_some());
        assert!(detector.detect("postal_code", &[]).is_some());
        assert!(detector.detect("latitude", &[]).is_some());
        assert!(detector.detect("company_name", &[]).is_some());
    }

    #[test]
    fn test_confidence_thresholds() {
        let detector = SemanticDetector::with_thresholds(0.9, 0.95);

        // This should have lower confidence, so may not pass
        let samples = vec![
            Some("maybe@email.com".to_string()),
            Some("not an email".to_string()),
        ];

        let result = detector.detect("contact", &samples);
        // With 50% match ratio, shouldn't detect
        assert!(result.is_none());
    }
}
