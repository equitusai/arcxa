//! Integration test for SHACL-DDL transformer
//!
//! Tests the complete pipeline:
//! 1. AsyncRdfStoreAdapter initialization (wraps in-memory RDF store)
//! 2. SHACL shape loading into RDF store
//! 3. SHACL-DDL transformer execution
//! 4. DDL generation for multiple SQL dialects

use anyhow::Result;
use graphica_coordinator::governance::{AsyncRdfStoreAdapter, GraphicaRdfStore};
use graphica_coordinator::workflows::engine::transformers::{ShaclDdlTransformer, Transformer};
use serde_json::json;
use std::sync::Arc;

/// Test SHACL shape in Turtle format
const TEST_SHACL_SHAPE: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
@prefix ex: <http://example.com/> .
@prefix schema: <http://schema.org/> .

ex:CustomerShape a sh:NodeShape ;
    sh:targetClass ex:Customer ;
    rdfs:label "Customer Entity Shape" ;
    sh:property [
        sh:path schema:identifier ;
        sh:name "id" ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxCount 1 ;
        sh:maxLength 50
    ] ;
    sh:property [
        sh:path schema:email ;
        sh:name "email" ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxLength 255 ;
        sh:pattern "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"
    ] ;
    sh:property [
        sh:path schema:name ;
        sh:name "name" ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
        sh:maxLength 200
    ] ;
    sh:property [
        sh:path schema:telephone ;
        sh:name "phone" ;
        sh:datatype xsd:string ;
        sh:maxLength 20
    ] .
"#;

#[tokio::test]
async fn test_shacl_ddl_transformer_with_in_memory_store() -> Result<()> {
    // Initialize in-memory RDF store (single source of truth)
    println!("🚀 Initializing in-memory RDF store...");
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);

    // Create async adapter for transformer use
    let rdf_adapter = Arc::new(AsyncRdfStoreAdapter::new(rdf_store.clone()));
    println!("✅ AsyncRdfStoreAdapter ready");

    // Load test SHACL shape into RDF store
    println!("📥 Loading SHACL shape into RDF store...");
    rdf_adapter.load_turtle(TEST_SHACL_SHAPE, None).await?;

    // Verify shape was loaded
    let count = rdf_adapter.count(None).await?;
    println!("✅ RDF store contains {} triples", count);
    assert!(count > 0, "No triples loaded");

    // Create SHACL-DDL transformer
    let transformer = ShaclDdlTransformer::new(rdf_adapter);

    // Test DDL generation for DB2
    println!("\n🔧 Testing DB2 DDL generation...");
    let mut data = json!({
        "shape_uri": "http://example.com/CustomerShape"
    });
    let config = json!({
        "dialect": "db2",
        "include_indexes": true,
        "include_foreign_keys": true
    });

    transformer.transform(&config, &mut data, None).await?;

    // Verify output structure
    assert!(data.get("table_name").is_some(), "Missing table_name");
    assert!(data.get("columns").is_some(), "Missing columns");
    assert!(data.get("ddl").is_some(), "Missing ddl");

    let table_name = data["table_name"].as_str().unwrap();
    println!("✅ Generated table: {}", table_name);

    let ddl = &data["ddl"];
    let create_table = ddl["create_table"].as_str().unwrap();
    println!("\n📄 Generated DDL:\n{}", create_table);

    // Verify DDL contains expected elements
    assert!(
        create_table.contains("CREATE TABLE"),
        "Missing CREATE TABLE"
    );
    assert!(create_table.contains("ID"), "Missing ID column");
    assert!(create_table.contains("EMAIL"), "Missing EMAIL column");
    assert!(create_table.contains("NAME"), "Missing NAME column");
    assert!(
        create_table.contains("NOT NULL"),
        "Missing NOT NULL constraints"
    );

    // Verify column list
    let columns = data["columns"].as_array().unwrap();
    println!("✅ Generated {} columns: {:?}", columns.len(), columns);
    assert!(columns.len() >= 3, "Expected at least 3 columns");

    println!("\n✅ SHACL-DDL transformer test passed!");

    Ok(())
}

#[tokio::test]
async fn test_shacl_ddl_transformer_config_validation() -> Result<()> {
    // Use in-memory RDF store for validation testing
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
    let rdf_adapter = Arc::new(AsyncRdfStoreAdapter::new(rdf_store));

    let transformer = ShaclDdlTransformer::new(rdf_adapter);

    // Test valid config
    let valid_config = json!({
        "dialect": "db2"
    });
    assert!(transformer.validate_config(&valid_config).is_ok());

    // Test invalid dialect
    let invalid_config = json!({
        "dialect": "invalid_dialect"
    });
    assert!(transformer.validate_config(&invalid_config).is_err());

    // Test valid alternative dialects
    for dialect in &["postgresql", "postgres", "oracle"] {
        let config = json!({"dialect": dialect});
        assert!(
            transformer.validate_config(&config).is_ok(),
            "Dialect '{}' should be valid",
            dialect
        );
    }

    println!("✅ Config validation tests passed!");

    Ok(())
}

#[test]
fn test_transformer_name() {
    // Use in-memory RDF store
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let rdf_adapter = Arc::new(AsyncRdfStoreAdapter::new(rdf_store));

    let transformer = ShaclDdlTransformer::new(rdf_adapter);
    assert_eq!(transformer.name(), "shacl_ddl_generator");
}

#[tokio::test]
async fn test_multiple_dialects() -> Result<()> {
    // Use in-memory RDF store
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory()?);
    let rdf_adapter = Arc::new(AsyncRdfStoreAdapter::new(rdf_store));

    let transformer = ShaclDdlTransformer::new(rdf_adapter);

    // Test that transformer accepts different dialect configs
    let dialects = vec!["db2", "postgresql", "oracle"];

    for dialect in dialects {
        let config = json!({
            "dialect": dialect,
            "include_indexes": true
        });

        // Validation should pass
        let validation_result = transformer.validate_config(&config);
        assert!(
            validation_result.is_ok(),
            "Dialect '{}' validation failed: {:?}",
            dialect,
            validation_result.err()
        );

        println!("✅ Dialect '{}' validated successfully", dialect);
    }

    println!("✅ Multiple dialect tests passed!");

    Ok(())
}
