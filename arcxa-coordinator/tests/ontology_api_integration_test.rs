//! Integration tests for Custom Ontology Management API
//!
//! These tests verify the end-to-end functionality of the ontology API endpoints.

use graphica_core::catalog::{OntologyRegistry, ValidationStatus};

/// Test ontology registration and validation
#[test]
fn test_ontology_registration_flow() {
    // Create registry
    let mut registry = OntologyRegistry::new();

    // Register a custom retail ontology
    let retail_ontology = r#"
@prefix retail: <http://example.com/retail#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

retail:Product a rdfs:Class ;
    rdfs:label "Product" ;
    rdfs:comment "A product in the retail catalog" .

retail:productSKU a rdf:Property ;
    rdfs:domain retail:Product ;
    rdfs:range xsd:string .

retail:productPrice a owl:DatatypeProperty ;
    rdfs:domain retail:Product ;
    rdfs:range xsd:decimal .
    "#;

    // Register the ontology
    let result = registry.register_custom_ontology(
        "retail_domain",
        retail_ontology,
        Some("http://example.com/retail#".to_string()),
    );

    // Verify registration succeeded
    assert!(result.is_ok(), "Ontology registration should succeed");
    let metadata = result.unwrap();
    assert_eq!(metadata.id, "retail_domain");
    assert_eq!(metadata.namespace, "http://example.com/retail#");

    // Verify ontology can be retrieved
    let ontology = registry.get_ontology("retail_domain");
    assert!(
        ontology.is_some(),
        "Registered ontology should be retrievable"
    );

    let ontology = ontology.unwrap();
    assert_eq!(ontology.metadata.id, "retail_domain");
    assert!(ontology.content.contains("retail:Product"));
    assert!(ontology.content.contains("retail:productSKU"));

    // Verify validation status
    match &ontology.validation_status {
        ValidationStatus::Valid | ValidationStatus::ValidWithWarnings { .. } => {
            // OK
        }
        _ => panic!("Ontology should be valid"),
    }
}

/// Test ontology listing and filtering
#[test]
fn test_ontology_listing() {
    let mut registry = OntologyRegistry::new();

    // Register multiple ontologies
    let ontology1 = r#"@prefix ont1: <http://example.com/ont1#> ."#;
    registry
        .register_custom_ontology("ont1", ontology1, None)
        .unwrap();

    let ontology2 = r#"@prefix ont2: <http://example.com/ont2#> ."#;
    registry
        .register_custom_ontology("ont2", ontology2, None)
        .unwrap();

    // List all ontologies
    let all = registry.list_ontologies();
    assert_eq!(all.len(), 2);

    // Deactivate one
    registry.deactivate_ontology("ont1").unwrap();

    // List active only
    let active = registry.list_active_ontologies();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "ont2");

    // List all should still show 2
    let all = registry.list_ontologies();
    assert_eq!(all.len(), 2);
}

/// Test ontology update
#[test]
fn test_ontology_update() {
    let mut registry = OntologyRegistry::new();

    // Register initial ontology
    let initial = r#"@prefix test: <http://example.com/test#> .
test:ClassA a rdfs:Class ."#;

    registry
        .register_custom_ontology("test", initial, None)
        .unwrap();

    // Update with new content
    let updated = r#"@prefix test: <http://example.com/test#> .
test:ClassA a rdfs:Class .
test:ClassB a rdfs:Class ."#;

    let result = registry.update_ontology("test", updated);
    assert!(result.is_ok(), "Update should succeed");

    // Verify updated content
    let ontology = registry.get_ontology("test").unwrap();
    assert!(ontology.content.contains("test:ClassB"));
}

/// Test merged ontology generation
#[test]
fn test_merged_ontology() {
    let mut registry = OntologyRegistry::new();

    // Register custom ontology
    let custom = r#"@prefix custom: <http://example.com/custom#> .
custom:MyClass a rdfs:Class ."#;

    registry
        .register_custom_ontology("custom1", custom, None)
        .unwrap();

    // Get merged ontology
    let merged = registry.get_merged_ontology();

    // Should contain base ontology
    assert!(merged.contains("@prefix gph:"));

    // Should contain extensions
    assert!(merged.contains("@prefix gphi:"));

    // Should contain custom ontology
    assert!(merged.contains("custom:MyClass"));
}

/// Test namespace conflict detection
#[test]
fn test_namespace_conflict() {
    let mut registry = OntologyRegistry::new();

    // Register first ontology with namespace
    let ont1 = r#"@prefix ont1: <http://example.com/shared#> ."#;
    registry
        .register_custom_ontology("ont1", ont1, Some("http://example.com/shared#".to_string()))
        .unwrap();

    // Try to register second with same namespace
    let ont2 = r#"@prefix ont2: <http://example.com/shared#> ."#;
    let result = registry.register_custom_ontology(
        "ont2",
        ont2,
        Some("http://example.com/shared#".to_string()),
    );

    // Should fail due to namespace conflict
    assert!(result.is_err(), "Should detect namespace conflict");
}

/// Test ontology statistics calculation
#[test]
fn test_ontology_statistics() {
    let mut registry = OntologyRegistry::new();

    let ontology = r#"
@prefix ex: <http://example.com#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

ex:Product a rdfs:Class .
ex:Customer a rdfs:Class .
ex:Order a rdfs:Class .

ex:hasProduct a rdf:Property .
ex:orderDate a rdf:Property .
    "#;

    registry
        .register_custom_ontology("test_stats", ontology, None)
        .unwrap();

    let registered = registry.get_ontology("test_stats").unwrap();

    // Verify statistics
    assert_eq!(registered.stats.class_count, 3, "Should count 3 classes");
    assert_eq!(
        registered.stats.property_count, 2,
        "Should count 2 properties"
    );
    assert!(registered.stats.size_bytes > 0, "Should calculate size");
}

/// Test ontology removal
#[test]
fn test_ontology_removal() {
    let mut registry = OntologyRegistry::new();

    // Register ontology
    let ont = r#"@prefix test: <http://example.com/test#> ."#;
    registry
        .register_custom_ontology("test", ont, None)
        .unwrap();

    // Verify it exists
    assert!(registry.get_ontology("test").is_some());

    // Remove it
    let removed = registry.remove_ontology("test");
    assert!(removed.is_ok(), "Removal should succeed");

    // Verify it's gone
    assert!(registry.get_ontology("test").is_none());

    // Verify namespace is freed
    let new_ont = r#"@prefix new: <http://example.com/test#> ."#;
    let result = registry.register_custom_ontology("new", new_ont, None);
    assert!(result.is_ok(), "Should reuse namespace after removal");
}

/// Test validation of invalid ontology
#[test]
fn test_invalid_ontology_validation() {
    let mut registry = OntologyRegistry::new();

    // Register empty ontology - it may succeed but be marked invalid
    let result = registry.register_custom_ontology("empty", "", None);

    // Either it fails registration, or succeeds with Invalid status
    match result {
        Err(_) => {
            // Failed registration - that's valid behavior
        }
        Ok(_) => {
            // If registration succeeded, check validation status
            let ontology = registry.get_ontology("empty").unwrap();
            match &ontology.validation_status {
                ValidationStatus::Invalid { .. } => {
                    // Correctly marked as invalid
                }
                _ => panic!("Empty ontology should be marked as invalid"),
            }
        }
    }
}

/// Test specific ontology merging
#[test]
fn test_specific_ontology_merge() {
    let mut registry = OntologyRegistry::new();

    // Register multiple ontologies
    registry
        .register_custom_ontology(
            "ont1",
            r#"@prefix ont1: <http://example.com/ont1#> ."#,
            None,
        )
        .unwrap();

    registry
        .register_custom_ontology(
            "ont2",
            r#"@prefix ont2: <http://example.com/ont2#> ."#,
            None,
        )
        .unwrap();

    registry
        .register_custom_ontology(
            "ont3",
            r#"@prefix ont3: <http://example.com/ont3#> ."#,
            None,
        )
        .unwrap();

    // Get merged ontology with only ont1 and ont3
    let merged = registry
        .get_merged_with_ontologies(&["ont1".to_string(), "ont3".to_string()])
        .unwrap();

    // Should contain ont1 and ont3
    assert!(merged.contains("ont1:"));
    assert!(merged.contains("ont3:"));

    // Should NOT contain ont2
    assert!(!merged.contains("ont2:"));
}
