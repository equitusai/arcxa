//! # Default Ontology Terms
//!
//! Fallback ontology terms from schema.org for when no custom ontologies are registered.
//!
//! ## Responsibilities
//!
//! - Provide a minimal set of common ontology terms
//! - Ensure field mapping always has baseline terms available
//! - Serve as examples for ontology term structure
//!
//! ## Usage
//!
//! These terms are automatically used when:
//! 1. No ontology registry is available
//! 2. Registry is empty (no active ontologies)
//! 3. All registered ontologies fail to parse

use crate::mapping::types::OntologyTerm;
use once_cell::sync::Lazy;

/// Default schema.org ontology terms (fallback)
///
/// These are commonly used terms that provide baseline mapping capabilities
/// even when no custom ontologies are registered.
pub static DEFAULT_TERMS: Lazy<Vec<OntologyTerm>> = Lazy::new(|| {
    vec![
        OntologyTerm {
            uri: "http://schema.org/email".to_string(),
            label: "Email".to_string(),
            description: Some("Email address".to_string()),
            parent_classes: vec!["http://schema.org/Text".to_string()],
            aliases: vec![
                "email".to_string(),
                "e-mail".to_string(),
                "emailAddress".to_string(),
            ],
            examples: vec!["user@example.com".to_string()],
            data_type: Some("VARCHAR".to_string()),
            value_patterns: vec![r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string()],
        },
        OntologyTerm {
            uri: "http://schema.org/name".to_string(),
            label: "Name".to_string(),
            description: Some("Person or entity name".to_string()),
            parent_classes: vec!["http://schema.org/Text".to_string()],
            aliases: vec![
                "name".to_string(),
                "fullName".to_string(),
                "personName".to_string(),
            ],
            examples: vec!["John Doe".to_string()],
            data_type: Some("VARCHAR".to_string()),
            value_patterns: vec![],
        },
        OntologyTerm {
            uri: "http://schema.org/identifier".to_string(),
            label: "Identifier".to_string(),
            description: Some("Unique identifier".to_string()),
            parent_classes: vec!["http://schema.org/Thing".to_string()],
            aliases: vec![
                "id".to_string(),
                "identifier".to_string(),
                "key".to_string(),
            ],
            examples: vec!["123456".to_string()],
            data_type: Some("INTEGER".to_string()),
            value_patterns: vec![],
        },
        OntologyTerm {
            uri: "http://schema.org/telephone".to_string(),
            label: "Telephone".to_string(),
            description: Some("Phone number".to_string()),
            parent_classes: vec!["http://schema.org/Text".to_string()],
            aliases: vec![
                "phone".to_string(),
                "telephone".to_string(),
                "phoneNumber".to_string(),
            ],
            examples: vec!["+1-555-1234".to_string()],
            data_type: Some("VARCHAR".to_string()),
            value_patterns: vec![
                r"^\+?\d{1,3}[-.\s]?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}$".to_string()
            ],
        },
    ]
});

/// Get a copy of the default ontology terms
pub fn get_default_terms() -> Vec<OntologyTerm> {
    DEFAULT_TERMS.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_terms_available() {
        let terms = get_default_terms();
        assert_eq!(terms.len(), 4);

        // Verify email term
        let email = terms
            .iter()
            .find(|t| t.uri == "http://schema.org/email")
            .unwrap();
        assert_eq!(email.label, "Email");
        assert!(email.aliases.contains(&"email".to_string()));
        assert!(!email.value_patterns.is_empty());
    }

    #[test]
    fn test_default_terms_have_labels() {
        for term in DEFAULT_TERMS.iter() {
            assert!(!term.label.is_empty(), "Term {} has no label", term.uri);
        }
    }

    #[test]
    fn test_default_terms_have_uris() {
        for term in DEFAULT_TERMS.iter() {
            assert!(term.uri.starts_with("http://"), "Invalid URI: {}", term.uri);
        }
    }
}
