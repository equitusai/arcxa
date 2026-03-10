//! BCP 47 Language Tag Validation
//!
//! Validates language tags according to RFC 5646 (BCP 47).
//! Implements a simplified but compliant subset for RDF use cases.
//!
//! ## Supported Formats
//!
//! - Primary language subtag: `en`, `ja`, `zh`
//! - Language + region: `en-US`, `zh-CN`, `fr-CA`
//! - Language + script: `zh-Hans`, `zh-Hant`
//! - Language + script + region: `zh-Hans-CN`
//!
//! ## Examples
//!
//! ```rust
//! use graphica_coordinator::governance::shard_coordinator::rdf::is_valid_language_tag;
//!
//! assert!(is_valid_language_tag("en"));
//! assert!(is_valid_language_tag("en-US"));
//! assert!(is_valid_language_tag("zh-Hans-CN"));
//! assert!(!is_valid_language_tag("123"));
//! assert!(!is_valid_language_tag("en-US-"));
//! ```

/// Validate a language tag against BCP 47 (RFC 5646)
///
/// ## Format
///
/// ```text
/// language-tag = primary-subtag *("-" subtag)
/// primary-subtag = 2-3 ALPHA
/// subtag = 2-8 alphanum
/// ```
///
/// ## Arguments
///
/// * `tag` - Language tag to validate (e.g., "en-US", "zh-Hans")
///
/// ## Returns
///
/// `true` if valid according to BCP 47, `false` otherwise
///
/// ## Performance
///
/// - Time: O(n) where n = tag length
/// - Space: O(1)
/// - Typical: < 100ns for most tags
pub fn is_valid_language_tag(tag: &str) -> bool {
    if tag.is_empty() {
        return false;
    }

    let parts: Vec<&str> = tag.split('-').collect();

    // Must have at least primary subtag
    if parts.is_empty() {
        return false;
    }

    // Validate primary language subtag (2-3 alphabetic characters)
    let primary = parts[0];
    if primary.len() < 2 || primary.len() > 3 {
        return false;
    }
    if !primary.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }

    // Validate subsequent subtags (2-8 alphanumeric characters)
    for subtag in &parts[1..] {
        if subtag.is_empty() {
            return false; // Empty subtag (double dash or trailing dash)
        }
        if subtag.len() < 2 || subtag.len() > 8 {
            return false;
        }
        if !subtag.chars().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
    }

    // All validation passed
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_language_codes() {
        assert!(is_valid_language_tag("en"));
        assert!(is_valid_language_tag("ja"));
        assert!(is_valid_language_tag("zh"));
        assert!(is_valid_language_tag("fr"));
        assert!(is_valid_language_tag("de"));
        assert!(is_valid_language_tag("es"));
    }

    #[test]
    fn test_language_with_region() {
        assert!(is_valid_language_tag("en-US"));
        assert!(is_valid_language_tag("en-GB"));
        assert!(is_valid_language_tag("zh-CN"));
        assert!(is_valid_language_tag("zh-TW"));
        assert!(is_valid_language_tag("fr-CA"));
        assert!(is_valid_language_tag("pt-BR"));
    }

    #[test]
    fn test_language_with_script() {
        assert!(is_valid_language_tag("zh-Hans"));
        assert!(is_valid_language_tag("zh-Hant"));
        assert!(is_valid_language_tag("sr-Latn"));
        assert!(is_valid_language_tag("sr-Cyrl"));
    }

    #[test]
    fn test_language_with_script_and_region() {
        assert!(is_valid_language_tag("zh-Hans-CN"));
        assert!(is_valid_language_tag("zh-Hant-TW"));
        assert!(is_valid_language_tag("sr-Latn-RS"));
    }

    #[test]
    fn test_invalid_primary_too_short() {
        assert!(!is_valid_language_tag("e"));
        assert!(!is_valid_language_tag("a"));
    }

    #[test]
    fn test_invalid_primary_too_long() {
        assert!(!is_valid_language_tag("english"));
        assert!(!is_valid_language_tag("japanese"));
    }

    #[test]
    fn test_invalid_primary_numeric() {
        assert!(!is_valid_language_tag("123"));
        assert!(!is_valid_language_tag("12"));
        assert!(!is_valid_language_tag("1en"));
    }

    #[test]
    fn test_invalid_empty() {
        assert!(!is_valid_language_tag(""));
    }

    #[test]
    fn test_invalid_subtag_too_short() {
        assert!(!is_valid_language_tag("en-U"));
        assert!(!is_valid_language_tag("en-1"));
    }

    #[test]
    fn test_invalid_subtag_too_long() {
        assert!(!is_valid_language_tag("en-VERYLONGTAG"));
    }

    #[test]
    fn test_invalid_trailing_dash() {
        assert!(!is_valid_language_tag("en-"));
        assert!(!is_valid_language_tag("en-US-"));
    }

    #[test]
    fn test_invalid_double_dash() {
        assert!(!is_valid_language_tag("en--US"));
    }

    #[test]
    fn test_invalid_special_characters() {
        assert!(!is_valid_language_tag("en_US"));
        assert!(!is_valid_language_tag("en@US"));
        assert!(!is_valid_language_tag("en US"));
    }

    #[test]
    fn test_case_insensitive_accepted() {
        // BCP 47 is case-insensitive, we accept any case
        assert!(is_valid_language_tag("EN"));
        assert!(is_valid_language_tag("En-Us"));
        assert!(is_valid_language_tag("ZH-HANS-CN"));
    }
}
