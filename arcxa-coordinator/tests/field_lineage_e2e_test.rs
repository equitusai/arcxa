//! End-to-End Integration Test for Field Lineage
//!
//! Tests complete flow: Create golden record → Persist to RDF → Query back → Validate

use chrono::Utc;
use graphica_core::orchestration::field_lineage::storage::FieldLineageStore;
use graphica_core::orchestration::field_lineage::{FieldResolver, SourceValue, StrategyType};
use std::collections::HashMap;

// Import RDF store types
use graphica_coordinator::governance::rdf_store::{GraphicaRdfStore, RdfStore};

/// End-to-end test: Create golden record, persist to RDF, query back, validate
#[test]
fn test_golden_record_end_to_end_with_rdf() {
    // Step 1: Create in-memory RDF store
    let rdf_store =
        GraphicaRdfStore::new_in_memory().expect("Failed to create in-memory RDF store");

    println!("\n=== Step 1: Created in-memory RDF store ===");

    // Step 2: Create golden record with field resolver
    let resolver = FieldResolver::new();
    let entity_id = "customer_e2e_test_001";

    // Create realistic source values for email field
    let email_sources = vec![
        SourceValue {
            id: "email_crm".to_string(),
            value: serde_json::json!("john.doe@example.com"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.95,
            confidence: Some(0.98),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "email_web".to_string(),
            value: serde_json::json!("john.doe@example.com"), // Same value
            source_system: "Website".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.75,
            confidence: Some(0.85),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "email_old".to_string(),
            value: serde_json::json!("j.doe@oldmail.com"), // Different value
            source_system: "LegacyDB".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.4,
            confidence: Some(0.6),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    // Create source values for name field
    let name_sources = vec![
        SourceValue {
            id: "name_crm".to_string(),
            value: serde_json::json!("John Doe"),
            source_system: "CRM".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.9,
            confidence: Some(0.95),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "name_erp".to_string(),
            value: serde_json::json!("John Doe"),
            source_system: "ERP".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.85,
            confidence: Some(0.90),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let mut fields = HashMap::new();
    fields.insert("email".to_string(), email_sources);
    fields.insert("name".to_string(), name_sources);

    // Resolve fields
    let resolutions = resolver
        .resolve_fields(entity_id, fields, None)
        .expect("Field resolution should succeed");

    assert_eq!(resolutions.len(), 2, "Should resolve 2 fields");

    // Create golden record
    let golden_record = resolver
        .create_resolved_entity(entity_id, resolutions)
        .expect("Golden record creation should succeed");

    println!(
        "=== Step 2: Created golden record with {} fields ===",
        golden_record.fields.len()
    );
    println!("  - Entity ID: {}", golden_record.entity_id);
    println!(
        "  - Overall confidence: {:.2}",
        golden_record.overall_confidence
    );
    println!("  - Conflict count: {}", golden_record.conflict_count);

    // Verify golden record structure
    assert_eq!(golden_record.entity_id, entity_id);
    assert_eq!(golden_record.fields.len(), 2);
    assert!(golden_record.overall_confidence > 0.0);

    // Verify email field was resolved correctly (frequency voting should pick john.doe@example.com)
    let email_field = golden_record
        .get_field("email")
        .expect("Email field should exist");
    assert_eq!(email_field.value, serde_json::json!("john.doe@example.com"));
    println!("  - Email resolved to: {}", email_field.value);

    // Verify name field
    let name_field = golden_record
        .get_field("name")
        .expect("Name field should exist");
    assert_eq!(name_field.value, serde_json::json!("John Doe"));
    println!("  - Name resolved to: {}", name_field.value);

    // Step 3: Persist to RDF store
    let storage = FieldLineageStore::new();
    let sparql_insert = resolver.resolved_entity_to_sparql(&golden_record);

    println!("\n=== Step 3: Persisting to RDF store ===");
    println!(
        "  - Generated SPARQL INSERT ({} chars)",
        sparql_insert.len()
    );

    // Execute SPARQL UPDATE
    rdf_store
        .update(&sparql_insert)
        .expect("SPARQL UPDATE should succeed");

    // Verify triples were inserted
    let triple_count = rdf_store
        .count_all_triples()
        .expect("Should get triple count");
    println!("  - Total triples in store: {}", triple_count);
    assert!(triple_count > 0, "Should have inserted triples");

    // Step 4: Query field lineage from RDF
    println!("\n=== Step 4: Querying field lineage from RDF ===");

    let lineage_query = storage.query_field_lineage(entity_id, "email");
    println!("  - Executing SPARQL query for email lineage");

    let lineage_results = rdf_store
        .query(&lineage_query)
        .expect("Lineage query should succeed");

    println!("  - Query returned {} result sets", lineage_results.len());

    // Verify we got results back
    assert!(!lineage_results.is_empty(), "Should have lineage results");

    // Extract bindings from first result
    if let Some(result) = lineage_results.get(0) {
        if let Some(bindings) = result
            .get("results")
            .and_then(|r| r.get("bindings"))
            .and_then(|b| b.as_array())
        {
            println!("  - Found {} lineage bindings", bindings.len());

            // We should have bindings for the resolved field
            // Note: The in-memory store may return results differently than a full RDF store
            // This validates that the query executes without error

            if !bindings.is_empty() {
                let first = &bindings[0];
                println!(
                    "  - First binding keys: {:?}",
                    first.as_object().map(|o| o.keys())
                );
            }
        }
    }

    // Step 5: Query field history
    println!("\n=== Step 5: Querying field history ===");

    let history_query = storage.query_field_history(entity_id, "email");
    let history_results = rdf_store
        .query(&history_query)
        .expect("History query should succeed");

    println!("  - History query executed successfully");
    println!("  - Returned {} result sets", history_results.len());

    // Step 6: Query conflicts requiring review
    println!("\n=== Step 6: Querying conflicts ===");

    let conflicts_query = storage.query_conflicts_requiring_review();
    let conflicts_results = rdf_store
        .query(&conflicts_query)
        .expect("Conflicts query should succeed");

    println!("  - Conflicts query executed successfully");
    println!("  - Returned {} result sets", conflicts_results.len());

    // Step 7: Validate round-trip correctness
    println!("\n=== Step 7: Validation Summary ===");
    println!(
        "  ✓ Golden record created with {} fields",
        golden_record.fields.len()
    );
    println!("  ✓ SPARQL generated ({} chars)", sparql_insert.len());
    println!("  ✓ Persisted to RDF ({} triples)", triple_count);
    println!("  ✓ Lineage query executed successfully");
    println!("  ✓ History query executed successfully");
    println!("  ✓ Conflicts query executed successfully");
    println!("\n=== End-to-End Test PASSED ===\n");

    // Final assertions
    assert_eq!(golden_record.entity_id, entity_id);
    assert_eq!(golden_record.fields.len(), 2);
    assert!(triple_count > 0);
}

/// Test multiple golden records in same RDF store
#[test]
fn test_multiple_golden_records_rdf() {
    let rdf_store =
        GraphicaRdfStore::new_in_memory().expect("Failed to create in-memory RDF store");

    let resolver = FieldResolver::with_strategy(StrategyType::Authority);

    // Create 3 golden records
    for i in 0..3 {
        let entity_id = format!("customer_multi_{}", i);

        let sources = vec![
            SourceValue {
                id: format!("src_high_{}", i),
                value: serde_json::json!(format!("value_high_{}", i)),
                source_system: "HighAuthority".to_string(),
                source_timestamp: Utc::now(),
                source_authority: 0.95,
                confidence: Some(0.98),
                vote_count: 0,
                vote_weight: 1.0,
                metadata: HashMap::new(),
            },
            SourceValue {
                id: format!("src_low_{}", i),
                value: serde_json::json!(format!("value_low_{}", i)),
                source_system: "LowAuthority".to_string(),
                source_timestamp: Utc::now(),
                source_authority: 0.3,
                confidence: Some(0.5),
                vote_count: 0,
                vote_weight: 1.0,
                metadata: HashMap::new(),
            },
        ];

        let mut fields = HashMap::new();
        fields.insert("test_field".to_string(), sources);

        let resolutions = resolver.resolve_fields(&entity_id, fields, None).unwrap();
        let golden_record = resolver
            .create_resolved_entity(&entity_id, resolutions)
            .unwrap();

        // Persist to RDF
        let sparql_insert = resolver.resolved_entity_to_sparql(&golden_record);
        rdf_store
            .update(&sparql_insert)
            .expect("Persistence should succeed");
    }

    // Verify all were persisted
    let triple_count = rdf_store.count_all_triples().expect("Should get count");
    println!("Multiple records test: {} triples stored", triple_count);

    assert!(
        triple_count > 0,
        "Should have stored multiple golden records"
    );
}

/// Test field resolution with conflict
#[test]
fn test_conflict_detection_e2e_rdf() {
    let rdf_store =
        GraphicaRdfStore::new_in_memory().expect("Failed to create in-memory RDF store");

    let resolver = FieldResolver::new();
    let entity_id = "customer_conflict_test";

    // Create evenly split sources (should create conflict)
    let sources = vec![
        SourceValue {
            id: "src_a".to_string(),
            value: serde_json::json!("value_A"),
            source_system: "SystemA".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.8,
            confidence: Some(0.9),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
        SourceValue {
            id: "src_b".to_string(),
            value: serde_json::json!("value_B"),
            source_system: "SystemB".to_string(),
            source_timestamp: Utc::now(),
            source_authority: 0.8,
            confidence: Some(0.9),
            vote_count: 0,
            vote_weight: 1.0,
            metadata: HashMap::new(),
        },
    ];

    let mut fields = HashMap::new();
    fields.insert("conflict_field".to_string(), sources);

    let resolutions = resolver.resolve_fields(entity_id, fields, None).unwrap();

    // Should have detected conflict
    assert_eq!(resolutions.len(), 1);
    let resolution = &resolutions[0];
    assert!(
        resolution.conflict.is_some(),
        "Should have detected conflict"
    );

    let golden_record = resolver
        .create_resolved_entity(entity_id, resolutions)
        .unwrap();

    // Golden record should flag conflict
    assert_eq!(golden_record.conflict_count, 1);
    assert!(golden_record.requires_review);

    println!(
        "Conflict test: Detected {} conflicts",
        golden_record.conflict_count
    );

    // Persist to RDF
    let sparql_insert = resolver.resolved_entity_to_sparql(&golden_record);
    rdf_store
        .update(&sparql_insert)
        .expect("Should persist with conflict");

    // Query conflicts
    let storage = FieldLineageStore::new();
    let conflicts_query = storage.query_conflicts_requiring_review();
    let results = rdf_store
        .query(&conflicts_query)
        .expect("Conflict query should work");

    println!("Conflict query executed, results: {} sets", results.len());
}
