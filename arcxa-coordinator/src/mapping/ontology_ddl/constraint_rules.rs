//! Ontology constraint registry
//!
//! Maps ontology terms to SHACL constraint templates.

use super::types::ShaclConstraintTemplate;
use crate::mapping::ontology_registry::RegistryClient;
use crate::mapping::types::OntologyTerm;
use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Registry mapping ontology terms to SHACL constraint templates
pub struct OntologyConstraintRegistry {
    rules: HashMap<String, ShaclConstraintTemplate>,
}

impl OntologyConstraintRegistry {
    /// Create registry with default schema.org rules
    pub fn new() -> Self {
        let mut rules = HashMap::new();

        // schema:email
        rules.insert(
            "http://schema.org/email".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/email".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                max_length: Some(255),
                pattern: Some(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string()),
                min_count: Some(1), // NOT NULL by default
                max_count: None,
                min_inclusive: None,
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: true,
                metadata: None,
            },
        );

        // schema:telephone
        rules.insert(
            "http://schema.org/telephone".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/telephone".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                max_length: Some(20),
                pattern: Some(r"^\+?[0-9\s\-\(\)]+$".to_string()),
                min_count: Some(0), // Nullable
                max_count: None,
                min_inclusive: None,
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        // schema:PostalAddress
        rules.insert(
            "http://schema.org/PostalAddress".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/PostalAddress".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                max_length: Some(500),
                pattern: None,
                min_count: Some(0),
                max_count: None,
                min_inclusive: None,
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        // schema:birthDate
        rules.insert(
            "http://schema.org/birthDate".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/birthDate".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#date".to_string(),
                max_length: None,
                pattern: None,
                min_count: Some(0),
                max_count: None,
                min_inclusive: Some(-62167219200.0), // 0001-01-01
                max_inclusive: None,                 // Current date would be computed at runtime
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        // schema:Person/age
        rules.insert(
            "http://schema.org/age".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/age".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#integer".to_string(),
                max_length: None,
                pattern: None,
                min_count: Some(0),
                max_count: None,
                min_inclusive: Some(0.0),
                max_inclusive: Some(150.0),
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        // schema:identifier
        rules.insert(
            "http://schema.org/identifier".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/identifier".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                max_length: Some(100),
                pattern: None,
                min_count: Some(1), // NOT NULL
                max_count: Some(1), // UNIQUE
                min_inclusive: None,
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: true,
                metadata: None,
            },
        );

        // schema:name
        rules.insert(
            "http://schema.org/name".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/name".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
                max_length: Some(255),
                pattern: None,
                min_count: Some(1), // NOT NULL
                max_count: None,
                min_inclusive: None,
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        // schema:price
        rules.insert(
            "http://schema.org/price".to_string(),
            ShaclConstraintTemplate {
                ontology_uri: "http://schema.org/price".to_string(),
                datatype: "http://www.w3.org/2001/XMLSchema#decimal".to_string(),
                max_length: None,
                pattern: None,
                min_count: Some(1),
                max_count: None,
                min_inclusive: Some(0.0), // Price >= 0
                max_inclusive: None,
                in_values: None,
                default_value: None,
                recommended_index: false,
                metadata: None,
            },
        );

        Self { rules }
    }

    /// Create registry with custom ontologies from RegistryClient
    ///
    /// This loads ontology terms from the PersistedOntologyRegistry and converts them
    /// to SHACL constraint templates. Custom ontology terms take precedence over
    /// default schema.org terms.
    ///
    /// # Arguments
    ///
    /// * `registry_client` - Client for querying custom ontologies
    ///
    /// # Returns
    ///
    /// Registry with both custom and default ontology constraints
    pub fn with_custom_ontologies(registry_client: &RegistryClient) -> Result<Self> {
        debug!("Creating OntologyConstraintRegistry with custom ontologies");

        // Start with default schema.org terms
        let mut registry = Self::new();
        let default_count = registry.rules.len();

        // Load custom ontology terms
        let custom_terms = registry_client
            .get_ontology_terms()
            .context("Failed to load ontology terms from registry")?;

        debug!("Loaded {} custom ontology terms", custom_terms.len());

        // Convert and register custom terms (overrides defaults)
        let mut custom_count = 0;
        let mut override_count = 0;

        for term in custom_terms {
            // Skip if this is already in defaults (don't override with generic conversion)
            let is_override = registry.rules.contains_key(&term.uri);

            // Convert OntologyTerm to ShaclConstraintTemplate
            match Self::term_to_constraint_template(&term) {
                Ok(template) => {
                    if is_override {
                        override_count += 1;
                        debug!("Overriding default constraint for: {}", term.uri);
                    } else {
                        custom_count += 1;
                    }
                    registry.register_constraint(template);
                }
                Err(e) => {
                    warn!("Failed to convert term {} to constraint: {}", term.uri, e);
                }
            }
        }

        info!(
            "OntologyConstraintRegistry initialized: {} default terms, {} custom terms, {} overrides",
            default_count, custom_count, override_count
        );

        Ok(registry)
    }

    /// Convert an OntologyTerm to a ShaclConstraintTemplate
    ///
    /// This performs a best-effort conversion from the generic OntologyTerm structure
    /// to a SHACL constraint template with SQL-compatible constraints.
    ///
    /// # Conversion Rules
    ///
    /// - URI → ontology_uri (direct mapping)
    /// - data_type → XSD datatype (with sensible defaults)
    /// - value_patterns → regex pattern (first pattern if multiple)
    /// - Defaults: nullable, no uniqueness, no indexing
    fn term_to_constraint_template(term: &OntologyTerm) -> Result<ShaclConstraintTemplate> {
        // Map common data types to XSD types
        let datatype = if let Some(dt) = &term.data_type {
            Self::map_data_type_to_xsd(dt)
        } else {
            // Default to string if no data type specified
            "http://www.w3.org/2001/XMLSchema#string".to_string()
        };

        // Use first pattern if available
        let pattern = if !term.value_patterns.is_empty() {
            Some(term.value_patterns[0].clone())
        } else {
            None
        };

        // Determine max_length for strings
        let max_length = if datatype.contains("string") {
            Some(255) // Conservative default
        } else {
            None
        };

        // Build constraint template with conservative defaults
        Ok(ShaclConstraintTemplate {
            ontology_uri: term.uri.clone(),
            datatype,
            max_length,
            pattern,
            min_count: Some(0), // Nullable by default for custom terms
            max_count: None,    // No uniqueness constraint by default
            min_inclusive: None,
            max_inclusive: None,
            in_values: None,
            default_value: None,
            recommended_index: false, // Don't recommend indexes by default
            metadata: Some(serde_json::json!({
                "label": term.label,
                "description": term.description,
                "aliases": term.aliases,
            })),
        })
    }

    /// Map data type strings to XSD URIs
    fn map_data_type_to_xsd(data_type: &str) -> String {
        let lower = data_type.to_lowercase();

        // If already a URI, return as-is
        if lower.starts_with("http://") || lower.starts_with("https://") {
            return data_type.to_string();
        }

        // Map common SQL/programming types to XSD
        match lower.as_str() {
            "string" | "varchar" | "text" | "char" => {
                "http://www.w3.org/2001/XMLSchema#string".to_string()
            }
            "integer" | "int" | "bigint" | "smallint" => {
                "http://www.w3.org/2001/XMLSchema#integer".to_string()
            }
            "decimal" | "numeric" | "number" => {
                "http://www.w3.org/2001/XMLSchema#decimal".to_string()
            }
            "double" | "float" | "real" => "http://www.w3.org/2001/XMLSchema#double".to_string(),
            "boolean" | "bool" => "http://www.w3.org/2001/XMLSchema#boolean".to_string(),
            "date" => "http://www.w3.org/2001/XMLSchema#date".to_string(),
            "datetime" | "timestamp" => "http://www.w3.org/2001/XMLSchema#dateTime".to_string(),
            "time" => "http://www.w3.org/2001/XMLSchema#time".to_string(),
            _ => {
                warn!(
                    "Unknown data type '{}', defaulting to xsd:string",
                    data_type
                );
                "http://www.w3.org/2001/XMLSchema#string".to_string()
            }
        }
    }

    /// Get constraint template for an ontology URI
    pub fn get_constraint(&self, ontology_uri: &str) -> Option<&ShaclConstraintTemplate> {
        self.rules.get(ontology_uri)
    }

    /// Register a custom constraint template
    pub fn register_constraint(&mut self, template: ShaclConstraintTemplate) {
        self.rules.insert(template.ontology_uri.clone(), template);
    }

    /// Get all registered ontology URIs
    pub fn get_all_uris(&self) -> Vec<String> {
        self.rules.keys().cloned().collect()
    }

    /// Check if an ontology URI has a constraint template
    pub fn has_constraint(&self, ontology_uri: &str) -> bool {
        self.rules.contains_key(ontology_uri)
    }
}

impl Default for OntologyConstraintRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_constraint() {
        let registry = OntologyConstraintRegistry::new();
        let constraint = registry.get_constraint("http://schema.org/email").unwrap();

        assert_eq!(constraint.max_length, Some(255));
        assert!(constraint.pattern.is_some());
        assert_eq!(constraint.min_count, Some(1)); // NOT NULL
        assert!(constraint.recommended_index);
        assert!(constraint.datatype.contains("string"));
    }

    #[test]
    fn test_telephone_nullable() {
        let registry = OntologyConstraintRegistry::new();
        let constraint = registry
            .get_constraint("http://schema.org/telephone")
            .unwrap();

        assert_eq!(constraint.min_count, Some(0)); // Nullable
        assert_eq!(constraint.max_length, Some(20));
        assert!(constraint.pattern.is_some());
    }

    #[test]
    fn test_age_range_constraint() {
        let registry = OntologyConstraintRegistry::new();
        let constraint = registry.get_constraint("http://schema.org/age").unwrap();

        assert_eq!(constraint.min_inclusive, Some(0.0));
        assert_eq!(constraint.max_inclusive, Some(150.0));
        assert!(constraint.datatype.contains("integer"));
    }

    #[test]
    fn test_identifier_unique() {
        let registry = OntologyConstraintRegistry::new();
        let constraint = registry
            .get_constraint("http://schema.org/identifier")
            .unwrap();

        assert_eq!(constraint.min_count, Some(1)); // NOT NULL
        assert_eq!(constraint.max_count, Some(1)); // UNIQUE
        assert!(constraint.recommended_index);
    }

    #[test]
    fn test_price_positive() {
        let registry = OntologyConstraintRegistry::new();
        let constraint = registry.get_constraint("http://schema.org/price").unwrap();

        assert_eq!(constraint.min_inclusive, Some(0.0));
        assert!(constraint.datatype.contains("decimal"));
    }

    #[test]
    fn test_custom_constraint_registration() {
        let mut registry = OntologyConstraintRegistry::new();

        let custom = ShaclConstraintTemplate {
            ontology_uri: "http://example.org/custom".to_string(),
            datatype: "http://www.w3.org/2001/XMLSchema#string".to_string(),
            max_length: Some(50),
            pattern: None,
            min_count: Some(1),
            max_count: None,
            min_inclusive: None,
            max_inclusive: None,
            in_values: None,
            default_value: None,
            recommended_index: false,
            metadata: None,
        };

        registry.register_constraint(custom);
        assert!(registry.has_constraint("http://example.org/custom"));
    }

    #[test]
    fn test_registry_has_all_default_terms() {
        let registry = OntologyConstraintRegistry::new();
        let uris = registry.get_all_uris();

        // Should have at least 8 default ontology terms
        assert!(uris.len() >= 8);

        // Check key terms are present
        assert!(registry.has_constraint("http://schema.org/email"));
        assert!(registry.has_constraint("http://schema.org/telephone"));
        assert!(registry.has_constraint("http://schema.org/age"));
        assert!(registry.has_constraint("http://schema.org/name"));
        assert!(registry.has_constraint("http://schema.org/price"));
    }

    #[test]
    fn test_term_to_constraint_template_string_type() {
        let term = OntologyTerm {
            uri: "http://example.com/CustomEmail".to_string(),
            label: "Custom Email".to_string(),
            description: Some("A custom email field".to_string()),
            parent_classes: vec![],
            aliases: vec!["email".to_string()],
            examples: vec!["user@example.com".to_string()],
            data_type: Some("string".to_string()),
            value_patterns: vec![r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$".to_string()],
        };

        let template = OntologyConstraintRegistry::term_to_constraint_template(&term).unwrap();

        assert_eq!(template.ontology_uri, "http://example.com/CustomEmail");
        assert_eq!(template.datatype, "http://www.w3.org/2001/XMLSchema#string");
        assert_eq!(template.max_length, Some(255));
        assert!(template.pattern.is_some());
        assert_eq!(template.min_count, Some(0)); // Nullable by default
    }

    #[test]
    fn test_term_to_constraint_template_integer_type() {
        let term = OntologyTerm {
            uri: "http://example.com/CustomAge".to_string(),
            label: "Custom Age".to_string(),
            description: None,
            parent_classes: vec![],
            aliases: vec![],
            examples: vec![],
            data_type: Some("integer".to_string()),
            value_patterns: vec![],
        };

        let template = OntologyConstraintRegistry::term_to_constraint_template(&term).unwrap();

        assert_eq!(
            template.datatype,
            "http://www.w3.org/2001/XMLSchema#integer"
        );
        assert_eq!(template.max_length, None); // No max_length for integers
    }

    #[test]
    fn test_map_data_type_to_xsd() {
        assert_eq!(
            OntologyConstraintRegistry::map_data_type_to_xsd("string"),
            "http://www.w3.org/2001/XMLSchema#string"
        );
        assert_eq!(
            OntologyConstraintRegistry::map_data_type_to_xsd("INTEGER"),
            "http://www.w3.org/2001/XMLSchema#integer"
        );
        assert_eq!(
            OntologyConstraintRegistry::map_data_type_to_xsd("decimal"),
            "http://www.w3.org/2001/XMLSchema#decimal"
        );
        assert_eq!(
            OntologyConstraintRegistry::map_data_type_to_xsd("date"),
            "http://www.w3.org/2001/XMLSchema#date"
        );
        assert_eq!(
            OntologyConstraintRegistry::map_data_type_to_xsd("boolean"),
            "http://www.w3.org/2001/XMLSchema#boolean"
        );

        // Unknown type should default to string
        let result = OntologyConstraintRegistry::map_data_type_to_xsd("unknown");
        assert_eq!(result, "http://www.w3.org/2001/XMLSchema#string");
    }

    #[test]
    fn test_with_custom_ontologies_integration() {
        use graphica_core::catalog::OntologyRegistry;
        use parking_lot::RwLock;
        use std::sync::Arc;

        // Create a test ontology registry with a custom term
        let mut ontology_registry = OntologyRegistry::new();
        let custom_ontology = r#"
            @prefix custom: <http://example.com/retail#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .

            custom:customerEmail a owl:DatatypeProperty ;
                rdfs:label "Customer Email" ;
                rdfs:comment "Email address for retail customers" .
        "#;

        ontology_registry
            .register_custom_ontology(
                "retail",
                custom_ontology,
                Some("http://example.com/retail#".to_string()),
            )
            .unwrap();

        // Create RegistryClient with the test registry
        let registry_client = RegistryClient::new(Some(Arc::new(RwLock::new(ontology_registry))));

        // Create OntologyConstraintRegistry with custom ontologies
        let constraint_registry =
            OntologyConstraintRegistry::with_custom_ontologies(&registry_client).unwrap();

        // Should have both default terms and custom term
        let uris = constraint_registry.get_all_uris();
        assert!(uris.len() > 8, "Should have default + custom terms");

        // Should have the custom term
        assert!(
            constraint_registry.has_constraint("http://example.com/retail#customerEmail"),
            "Should have custom term from ontology"
        );

        // Should still have default terms
        assert!(constraint_registry.has_constraint("http://schema.org/email"));
    }
}
