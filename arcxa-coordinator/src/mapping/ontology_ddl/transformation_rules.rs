//! Ontology-Driven Transformation Rules
//!
//! Maps ontology terms to standard data transformations, enabling automatic
//! data cleansing and normalization based on semantic types.
//!
//! ## Example
//!
//! - `schema:email` → `[TRIM, LOWER]` (normalize email addresses)
//! - `schema:givenName` → `[TRIM, PROPER_CASE]` (proper case for names)
//! - `schema:telephone` → `[REGEX: strip non-digits, FORMAT: (XXX) XXX-XXXX]`

use anyhow::{Context, Result};
use std::collections::HashMap;

/// Transformation rule for a field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldTransformation {
    /// Field name
    pub field_name: String,

    /// Transformation expression (e.g., "TRIM(LOWER({field}))")
    pub expression: String,

    /// Ontology URI that triggered this transformation
    pub ontology_uri: String,

    /// Description of what the transformation does
    pub description: String,
}

/// Registry of ontology→transformation mappings
pub struct OntologyTransformationRegistry {
    /// Map from ontology URI to transformation template
    rules: HashMap<String, TransformationTemplate>,
}

/// Template for generating transformations from ontology
#[derive(Debug, Clone)]
struct TransformationTemplate {
    /// Transformation expression template (use {field} as placeholder)
    expression: String,

    /// Description of the transformation
    description: String,

    /// Whether to always apply this transformation
    always_apply: bool,
}

impl OntologyTransformationRegistry {
    /// Create a new registry with default transformation rules
    pub fn new() -> Self {
        let mut rules = HashMap::new();

        // Email normalization
        rules.insert(
            "http://schema.org/email".to_string(),
            TransformationTemplate {
                expression: "LOWER(TRIM({field}))".to_string(),
                description: "Normalize email: trim whitespace and convert to lowercase"
                    .to_string(),
                always_apply: true,
            },
        );

        // Name fields - proper case
        rules.insert(
            "http://schema.org/givenName".to_string(),
            TransformationTemplate {
                expression: "PROPER_CASE(TRIM({field}))".to_string(),
                description: "Normalize name: trim whitespace and proper case".to_string(),
                always_apply: true,
            },
        );
        rules.insert(
            "http://schema.org/familyName".to_string(),
            TransformationTemplate {
                expression: "PROPER_CASE(TRIM({field}))".to_string(),
                description: "Normalize name: trim whitespace and proper case".to_string(),
                always_apply: true,
            },
        );
        rules.insert(
            "http://schema.org/name".to_string(),
            TransformationTemplate {
                expression: "TRIM({field})".to_string(),
                description: "Normalize name: trim whitespace".to_string(),
                always_apply: true,
            },
        );

        // Telephone - strip non-digits
        rules.insert(
            "http://schema.org/telephone".to_string(),
            TransformationTemplate {
                expression: "REGEX_REPLACE({field}, '[^0-9]', '')".to_string(),
                description: "Normalize phone: strip non-digits".to_string(),
                always_apply: true,
            },
        );

        // URL - trim and lowercase
        rules.insert(
            "http://schema.org/url".to_string(),
            TransformationTemplate {
                expression: "LOWER(TRIM({field}))".to_string(),
                description: "Normalize URL: trim whitespace and lowercase".to_string(),
                always_apply: true,
            },
        );

        // Address components - trim
        rules.insert(
            "http://schema.org/streetAddress".to_string(),
            TransformationTemplate {
                expression: "TRIM({field})".to_string(),
                description: "Normalize address: trim whitespace".to_string(),
                always_apply: true,
            },
        );
        rules.insert(
            "http://schema.org/postalCode".to_string(),
            TransformationTemplate {
                expression: "UPPER(TRIM({field}))".to_string(),
                description: "Normalize postal code: trim and uppercase".to_string(),
                always_apply: true,
            },
        );

        // Identifiers - trim
        rules.insert(
            "http://schema.org/identifier".to_string(),
            TransformationTemplate {
                expression: "TRIM({field})".to_string(),
                description: "Normalize identifier: trim whitespace".to_string(),
                always_apply: true,
            },
        );

        Self { rules }
    }

    /// Get transformation for an ontology URI
    pub fn get_transformation(
        &self,
        ontology_uri: &str,
        field_name: &str,
    ) -> Option<FieldTransformation> {
        self.rules.get(ontology_uri).map(|template| {
            let expression = template.expression.replace("{field}", field_name);
            FieldTransformation {
                field_name: field_name.to_string(),
                expression,
                ontology_uri: ontology_uri.to_string(),
                description: template.description.clone(),
            }
        })
    }

    /// Check if an ontology URI has transformation rules
    pub fn has_transformation(&self, ontology_uri: &str) -> bool {
        self.rules.contains_key(ontology_uri)
    }

    /// Get all ontology URIs with transformation rules
    pub fn get_all_uris(&self) -> Vec<String> {
        self.rules.keys().cloned().collect()
    }

    /// Register a custom transformation rule
    pub fn register_rule(&mut self, ontology_uri: String, expression: String, description: String) {
        self.rules.insert(
            ontology_uri,
            TransformationTemplate {
                expression,
                description,
                always_apply: true,
            },
        );
    }
}

impl Default for OntologyTransformationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_transformation() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry
            .get_transformation("http://schema.org/email", "email")
            .unwrap();

        assert_eq!(transformation.field_name, "email");
        assert_eq!(transformation.expression, "LOWER(TRIM(email))");
        assert_eq!(transformation.ontology_uri, "http://schema.org/email");
        assert!(transformation.description.contains("email"));
    }

    #[test]
    fn test_name_transformation() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry
            .get_transformation("http://schema.org/givenName", "first_name")
            .unwrap();

        assert_eq!(transformation.field_name, "first_name");
        assert_eq!(transformation.expression, "PROPER_CASE(TRIM(first_name))");
    }

    #[test]
    fn test_telephone_transformation() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry
            .get_transformation("http://schema.org/telephone", "phone")
            .unwrap();

        assert_eq!(transformation.field_name, "phone");
        assert_eq!(
            transformation.expression,
            "REGEX_REPLACE(phone, '[^0-9]', '')"
        );
    }

    #[test]
    fn test_has_transformation() {
        let registry = OntologyTransformationRegistry::new();

        assert!(registry.has_transformation("http://schema.org/email"));
        assert!(registry.has_transformation("http://schema.org/telephone"));
        assert!(!registry.has_transformation("http://schema.org/unknownField"));
    }

    #[test]
    fn test_get_all_uris() {
        let registry = OntologyTransformationRegistry::new();
        let uris = registry.get_all_uris();

        assert!(uris.len() >= 8);
        assert!(uris.contains(&"http://schema.org/email".to_string()));
        assert!(uris.contains(&"http://schema.org/telephone".to_string()));
    }

    #[test]
    fn test_register_custom_rule() {
        let mut registry = OntologyTransformationRegistry::new();

        registry.register_rule(
            "http://example.com/customField".to_string(),
            "UPPER({field})".to_string(),
            "Convert to uppercase".to_string(),
        );

        let transformation = registry
            .get_transformation("http://example.com/customField", "test_field")
            .unwrap();

        assert_eq!(transformation.expression, "UPPER(test_field)");
        assert_eq!(transformation.description, "Convert to uppercase");
    }

    #[test]
    fn test_no_transformation_for_unmapped_uri() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry.get_transformation("http://schema.org/age", "age");

        assert!(transformation.is_none(), "age has no transformation rule");
    }

    #[test]
    fn test_field_placeholder_replacement() {
        let registry = OntologyTransformationRegistry::new();

        // Test with different field names
        let trans1 = registry
            .get_transformation("http://schema.org/email", "user_email")
            .unwrap();
        assert_eq!(trans1.expression, "LOWER(TRIM(user_email))");

        let trans2 = registry
            .get_transformation("http://schema.org/email", "contact_email")
            .unwrap();
        assert_eq!(trans2.expression, "LOWER(TRIM(contact_email))");
    }

    #[test]
    fn test_postal_code_transformation() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry
            .get_transformation("http://schema.org/postalCode", "zip")
            .unwrap();

        assert_eq!(transformation.expression, "UPPER(TRIM(zip))");
    }

    #[test]
    fn test_url_transformation() {
        let registry = OntologyTransformationRegistry::new();
        let transformation = registry
            .get_transformation("http://schema.org/url", "website")
            .unwrap();

        assert_eq!(transformation.expression, "LOWER(TRIM(website))");
    }
}
