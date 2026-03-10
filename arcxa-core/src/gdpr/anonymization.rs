//! Anonymization Strategies for GDPR Compliance
//!
//! Implements various anonymization techniques as an alternative to hard deletion.
//! When data must be retained for statistical/analytical purposes but personal
//! identifiers must be removed, anonymization provides a GDPR-compliant solution.
//!
//! ## Anonymization Techniques
//!
//! 1. **Hash**: Replace identifiers with cryptographic hashes
//!    - Pros: Deterministic, allows grouping by same identifier
//!    - Cons: Subject to rainbow table attacks if salt not used
//!
//! 2. **Mask**: Replace sensitive data with placeholder characters
//!    - Pros: Preserves data format/structure
//!    - Cons: May leak information (e.g., email domain, phone area code)
//!
//! 3. **Generalize**: Replace specific values with broader categories
//!    - Pros: Preserves statistical utility
//!    - Cons: May still allow re-identification if categories too narrow
//!
//! 4. **Noise Addition**: Add statistical noise to numeric values
//!    - Pros: Preserves aggregate statistics
//!    - Cons: Reduces precision of individual records
//!
//! 5. **Synthetic**: Replace with synthetic data from same distribution
//!    - Pros: Maintains statistical properties
//!    - Cons: Complex to implement correctly

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Anonymization Strategy
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnonymizationStrategy {
    /// Hash with SHA-256 (optionally salted)
    Hash {
        /// Salt to prevent rainbow table attacks
        salt: Option<String>,
    },

    /// Mask with placeholder characters
    Mask {
        /// Character to use for masking (default: '*')
        mask_char: char,
        /// How many characters to leave unmasked (e.g., for email domain)
        preserve_last: usize,
    },

    /// Generalize to broader category
    Generalize {
        /// Generalization rules (e.g., age -> age range, zip -> region)
        rules: GeneralizationRules,
    },

    /// Replace with random UUID
    RandomUuid,

    /// Replace with sequential counter
    Sequential {
        /// Prefix for the counter (e.g., "user_")
        prefix: String,
    },
}

impl Default for AnonymizationStrategy {
    fn default() -> Self {
        AnonymizationStrategy::Hash { salt: None }
    }
}

/// Generalization Rules
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeneralizationRules {
    /// Age generalization: replace age with age range
    AgeRange {
        /// Range size in years (e.g., 10 for age ranges: 0-10, 10-20, etc.)
        range_size: u32,
    },

    /// Geographic generalization: replace specific location with region
    Geographic {
        /// Generalization level: zip -> city -> state -> country
        level: GeographicLevel,
    },

    /// Date generalization: replace date with month/year
    DateTruncation {
        /// Truncate to: day, month, quarter, year
        level: DateLevel,
    },

    /// Custom generalization function name
    Custom(String),
}

/// Geographic generalization level
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographicLevel {
    /// Keep only country
    Country,
    /// Keep state/province
    State,
    /// Keep city
    City,
    /// Keep first 3 digits of postal code
    PostalPrefix,
}

/// Date truncation level
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateLevel {
    /// Truncate to year
    Year,
    /// Truncate to quarter
    Quarter,
    /// Truncate to month
    Month,
    /// Truncate to week
    Week,
}

/// Anonymizer
///
/// Applies anonymization strategies to sensitive data
pub struct Anonymizer {
    /// Strategy to use
    strategy: AnonymizationStrategy,

    /// Counter for sequential anonymization
    counter: std::sync::atomic::AtomicU64,
}

impl Anonymizer {
    /// Create new anonymizer with specified strategy
    pub fn new(strategy: AnonymizationStrategy) -> Self {
        Self {
            strategy,
            counter: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// Create default anonymizer (SHA-256 hash without salt)
    pub fn default_hash() -> Self {
        Self::new(AnonymizationStrategy::Hash { salt: None })
    }

    /// Create hash-based anonymizer with salt
    pub fn with_salt(salt: impl Into<String>) -> Self {
        Self::new(AnonymizationStrategy::Hash {
            salt: Some(salt.into()),
        })
    }

    /// Create mask-based anonymizer
    pub fn with_mask(mask_char: char, preserve_last: usize) -> Self {
        Self::new(AnonymizationStrategy::Mask {
            mask_char,
            preserve_last,
        })
    }

    /// Anonymize a string value
    pub fn anonymize(&self, value: &str) -> String {
        match &self.strategy {
            AnonymizationStrategy::Hash { salt } => {
                let mut hasher = Sha256::new();
                hasher.update(value.as_bytes());

                if let Some(salt) = salt {
                    hasher.update(salt.as_bytes());
                }

                format!("{:x}", hasher.finalize())
            }

            AnonymizationStrategy::Mask {
                mask_char,
                preserve_last,
            } => {
                let len = value.len();
                if len <= *preserve_last {
                    // If value is shorter than preserve_last, mask it all
                    mask_char.to_string().repeat(len)
                } else {
                    let mask_count = len - preserve_last;
                    let masked = mask_char.to_string().repeat(mask_count);
                    let preserved = &value[mask_count..];
                    format!("{}{}", masked, preserved)
                }
            }

            AnonymizationStrategy::Generalize { rules } => self.generalize(value, rules),

            AnonymizationStrategy::RandomUuid => Uuid::new_v4().to_string(),

            AnonymizationStrategy::Sequential { prefix } => {
                let count = self
                    .counter
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                format!("{}{}", prefix, count)
            }
        }
    }

    /// Apply generalization rules
    fn generalize(&self, value: &str, rules: &GeneralizationRules) -> String {
        match rules {
            GeneralizationRules::AgeRange { range_size } => {
                // Try to parse as age and generalize to range
                if let Ok(age) = value.parse::<u32>() {
                    let range_start = (age / range_size) * range_size;
                    let range_end = range_start + range_size;
                    format!("{}-{}", range_start, range_end)
                } else {
                    // If not a valid age, return masked value
                    "**REDACTED**".to_string()
                }
            }

            GeneralizationRules::Geographic { level } => {
                match level {
                    GeographicLevel::PostalPrefix => {
                        // Keep first 3 characters of postal code
                        if value.len() >= 3 {
                            format!("{}**", &value[..3])
                        } else {
                            "***".to_string()
                        }
                    }
                    _ => {
                        // For other geographic levels, would need geocoding service
                        "**REGION**".to_string()
                    }
                }
            }

            GeneralizationRules::DateTruncation { level } => {
                // Parse date and truncate to specified level
                // For now, simplified implementation
                match level {
                    DateLevel::Year => {
                        if value.len() >= 4 {
                            value[..4].to_string()
                        } else {
                            "****".to_string()
                        }
                    }
                    DateLevel::Month => {
                        if value.len() >= 7 {
                            value[..7].to_string()
                        } else {
                            "****-**".to_string()
                        }
                    }
                    _ => "**REDACTED**".to_string(),
                }
            }

            GeneralizationRules::Custom(func_name) => {
                // Custom functions would be registered separately
                format!("**CUSTOM:{}**", func_name)
            }
        }
    }

    /// Anonymize email address
    ///
    /// Special handling for emails: hash local part, optionally preserve domain
    pub fn anonymize_email(&self, email: &str, preserve_domain: bool) -> String {
        if let Some(at_pos) = email.find('@') {
            let (local, domain) = email.split_at(at_pos);
            let anonymized_local = self.anonymize(local);

            if preserve_domain {
                format!("{}@{}", anonymized_local, &domain[1..])
            } else {
                format!("{}@example.com", anonymized_local)
            }
        } else {
            // Not a valid email, anonymize as regular string
            self.anonymize(email)
        }
    }

    /// Anonymize phone number
    ///
    /// Special handling for phone numbers: optionally preserve country/area code
    pub fn anonymize_phone(&self, phone: &str, preserve_area_code: bool) -> String {
        // Remove non-digit characters
        let digits: String = phone.chars().filter(|c| c.is_numeric()).collect();

        if preserve_area_code && digits.len() >= 6 {
            // Preserve first 3 digits (area code), mask the rest
            let area_code = &digits[..3];
            let masked_rest = "*".repeat(digits.len() - 3);
            format!("{}-{}", area_code, masked_rest)
        } else {
            "*".repeat(digits.len().max(10))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_anonymization() {
        let anonymizer = Anonymizer::default_hash();
        let anonymized = anonymizer.anonymize("user@example.com");

        // Hash should be deterministic
        assert_eq!(anonymized, anonymizer.anonymize("user@example.com"));

        // Hash should be different for different inputs
        assert_ne!(anonymized, anonymizer.anonymize("other@example.com"));
    }

    #[test]
    fn test_hash_with_salt() {
        let anonymizer1 = Anonymizer::with_salt("salt1");
        let anonymizer2 = Anonymizer::with_salt("salt2");

        // Same input with different salts should produce different hashes
        assert_ne!(
            anonymizer1.anonymize("user@example.com"),
            anonymizer2.anonymize("user@example.com")
        );
    }

    #[test]
    fn test_mask_anonymization() {
        let anonymizer = Anonymizer::with_mask('*', 4);

        // Mask all but last 4 characters
        assert_eq!(
            anonymizer.anonymize("user@example.com"),
            "************m.com"
        );

        // If shorter than preserve_last, mask all
        assert_eq!(anonymizer.anonymize("abc"), "***");
    }

    #[test]
    fn test_uuid_anonymization() {
        let anonymizer = Anonymizer::new(AnonymizationStrategy::RandomUuid);

        let uuid1 = anonymizer.anonymize("user@example.com");
        let uuid2 = anonymizer.anonymize("user@example.com");

        // UUIDs should be random, not deterministic
        assert_ne!(uuid1, uuid2);

        // Both should be valid UUIDs
        assert!(Uuid::parse_str(&uuid1).is_ok());
        assert!(Uuid::parse_str(&uuid2).is_ok());
    }

    #[test]
    fn test_sequential_anonymization() {
        let anonymizer = Anonymizer::new(AnonymizationStrategy::Sequential {
            prefix: "user_".to_string(),
        });

        assert_eq!(anonymizer.anonymize("alice"), "user_0");
        assert_eq!(anonymizer.anonymize("bob"), "user_1");
        assert_eq!(anonymizer.anonymize("charlie"), "user_2");
    }

    #[test]
    fn test_age_generalization() {
        let anonymizer = Anonymizer::new(AnonymizationStrategy::Generalize {
            rules: GeneralizationRules::AgeRange { range_size: 10 },
        });

        assert_eq!(anonymizer.anonymize("25"), "20-30");
        assert_eq!(anonymizer.anonymize("5"), "0-10");
        assert_eq!(anonymizer.anonymize("42"), "40-50");
    }

    #[test]
    fn test_postal_code_generalization() {
        let anonymizer = Anonymizer::new(AnonymizationStrategy::Generalize {
            rules: GeneralizationRules::Geographic {
                level: GeographicLevel::PostalPrefix,
            },
        });

        assert_eq!(anonymizer.anonymize("94105"), "941**");
        assert_eq!(anonymizer.anonymize("12345-6789"), "123**");
    }

    #[test]
    fn test_email_anonymization() {
        let anonymizer = Anonymizer::default_hash();

        // Preserve domain
        let anonymized = anonymizer.anonymize_email("user@example.com", true);
        assert!(anonymized.ends_with("@example.com"));
        assert!(!anonymized.starts_with("user"));

        // Don't preserve domain
        let anonymized = anonymizer.anonymize_email("user@example.com", false);
        assert!(anonymized.ends_with("@example.com"));
    }

    #[test]
    fn test_phone_anonymization() {
        let anonymizer = Anonymizer::default_hash();

        // Preserve area code
        let anonymized = anonymizer.anonymize_phone("(555) 123-4567", true);
        assert!(anonymized.starts_with("555"));
        assert!(anonymized.contains("*"));

        // Don't preserve area code
        let anonymized = anonymizer.anonymize_phone("(555) 123-4567", false);
        assert_eq!(anonymized, "**********");
    }
}
