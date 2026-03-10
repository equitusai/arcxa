//! URI Utilities
//!
//! Shared utilities for working with RDF URIs and extracting local names.

use anyhow::{anyhow, Result};

/// Extract the local name from an RDF URI
///
/// Handles common URI patterns:
/// - `http://schema.org/Person` → `Person`
/// - `retail:customerFirstName` → `customerFirstName`
/// - `http://example.com/ontology#firstName` → `firstName`
/// - `retail:` → None (empty local name)
///
/// ## Examples
///
/// ```
/// use graphica_coordinator::mapping::uri_utils::extract_local_name;
///
/// assert_eq!(extract_local_name("http://schema.org/Person"), Some("Person".to_string()));
/// assert_eq!(extract_local_name("retail:customerFirstName"), Some("customerFirstName".to_string()));
/// assert_eq!(extract_local_name("http://example.com/ontology#firstName"), Some("firstName".to_string()));
/// assert_eq!(extract_local_name("retail:"), None); // No local name
/// assert_eq!(extract_local_name("firstName"), Some("firstName".to_string()));
/// ```
pub fn extract_local_name(uri: &str) -> Option<String> {
    uri.rsplit(['#', '/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Extract the local name from an RDF URI or return an error
///
/// Same as `extract_local_name()` but returns an error instead of None.
/// Useful in contexts where an empty local name is unexpected.
pub fn extract_local_name_or_err(uri: &str) -> Result<String> {
    extract_local_name(uri).ok_or_else(|| anyhow!("URI has no local name: {}", uri))
}

/// Extract namespace from a URI
///
/// Returns the namespace prefix from URIs like:
/// - `http://schema.org/Person` → `http://schema.org/`
/// - `retail:customerFirstName` → `retail:`
/// - `http://example.com/ontology#firstName` → `http://example.com/ontology#`
pub fn extract_namespace(uri: &str) -> Option<String> {
    if let Some(pos) = uri.rfind(['#', '/', ':']) {
        Some(uri[..=pos].to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_local_name() {
        // Standard HTTP URIs
        assert_eq!(
            extract_local_name("http://schema.org/Person"),
            Some("Person".to_string())
        );
        assert_eq!(
            extract_local_name("http://example.com/ontology#firstName"),
            Some("firstName".to_string())
        );

        // Prefixed names
        assert_eq!(
            extract_local_name("retail:customerFirstName"),
            Some("customerFirstName".to_string())
        );
        assert_eq!(extract_local_name("foaf:name"), Some("name".to_string()));

        // Nested colons (use rightmost)
        assert_eq!(
            extract_local_name("ns:nested:property"),
            Some("property".to_string())
        );

        // Edge cases
        assert_eq!(extract_local_name("retail:"), None); // Empty local name
        assert_eq!(extract_local_name("http://example.com/"), None); // Trailing slash
        assert_eq!(
            extract_local_name("firstName"),
            Some("firstName".to_string())
        ); // No separator
    }

    #[test]
    fn test_extract_local_name_or_err() {
        // Success cases
        assert!(extract_local_name_or_err("http://schema.org/Person").is_ok());
        assert_eq!(
            extract_local_name_or_err("retail:customerFirstName").unwrap(),
            "customerFirstName"
        );

        // Error cases
        assert!(extract_local_name_or_err("retail:").is_err());
        assert!(extract_local_name_or_err("http://example.com/").is_err());
    }

    #[test]
    fn test_extract_namespace() {
        assert_eq!(
            extract_namespace("http://schema.org/Person"),
            Some("http://schema.org/".to_string())
        );
        assert_eq!(
            extract_namespace("retail:customerFirstName"),
            Some("retail:".to_string())
        );
        assert_eq!(
            extract_namespace("http://example.com/ontology#firstName"),
            Some("http://example.com/ontology#".to_string())
        );
        assert_eq!(extract_namespace("firstName"), None);
    }
}
