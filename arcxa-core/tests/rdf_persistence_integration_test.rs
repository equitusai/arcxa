//! Integration test for Phase 2.1: RDF Triple Persistence
//!
//! Tests end-to-end semantic enrichment → RDF persistence pipeline

use graphica_core::checkpointing::CheckpointableDedupState;
use graphica_core::core::lineage::DataRef;
use graphica_core::inference::rdf_store::{RdfStore, RdfStoreConfig};
use graphica_core::inference::semantic::ColumnNameDetector;
use graphica_core::ingestion::{build_graphica_flow, Record};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tempfile::TempDir;
use timely::dataflow::operators::Inspect;
use timely::Config;

#[test]
fn test_end_to_end_rdf_persistence() {
    // Setup: Create temp directory for RDF files
    let temp_dir = TempDir::new().unwrap();
    let rdf_config = RdfStoreConfig {
        base_dir: temp_dir.path().to_path_buf(),
        separate_files: true,
        flush_frequency: 1, // Flush immediately for testing
    };

    let dedup_state = Arc::new(CheckpointableDedupState::new(60_000, 100_000));
    let detector = Arc::new(ColumnNameDetector::new());
    let rdf_store = Arc::new(RdfStore::new(rdf_config, "test-source").unwrap());
    let results = Arc::new(Mutex::new(Vec::new()));

    let results_clone = results.clone();
    let rdf_clone = rdf_store.clone();

    timely::execute(Config::thread(), move |worker| {
        let dedup = (*dedup_state).clone();
        let det = detector.clone();
        let store = rdf_clone.clone();
        let res = results_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, output, _resolved_entity_stream) = build_graphica_flow(
                scope,
                dedup.clone(),
                Some(det.clone()),
                Some(store.clone()),
                None, // No resolved entity config for this test
            );

            // Test record 1: Customer with email and phone
            let record1 = Record {
                id: "customer-001".to_string(),
                dataset: "customers".to_string(),
                data: json!({
                    "email": "alice@example.com",
                    "phone_number": "555-0001",
                    "full_name": "Alice Johnson",
                    "billing_address": "123 Main St",
                }),
                source: DataRef {
                    system: "test-cdc".to_string(),
                    path: "customers".to_string(),
                    version: Some("1.0".to_string()),
                    extracted_at: chrono::Utc::now(),
                    cdc_position: None,
                },
                timestamp: 1000000,
                tenant_id: "test-tenant".to_string(),
                semantic_metadata: None,
            };

            // Test record 2: Product with SKU and price
            let record2 = Record {
                id: "product-001".to_string(),
                dataset: "products".to_string(),
                data: json!({
                    "sku": "WIDGET-123",
                    "product_name": "Super Widget",
                    "price_usd": "29.99",
                    "created_at": "2024-01-15T10:00:00Z",
                }),
                source: DataRef {
                    system: "test-cdc".to_string(),
                    path: "products".to_string(),
                    version: Some("1.0".to_string()),
                    extracted_at: chrono::Utc::now(),
                    cdc_position: None,
                },
                timestamp: 2000000,
                tenant_id: "test-tenant".to_string(),
                semantic_metadata: None,
            };

            input.send(record1);
            input.send(record2);
            input.advance_to(1);

            // Collect results
            let res_inner = res.clone();
            output.inspect(move |processed| {
                res_inner.lock().unwrap().push(processed.clone());
            });
        });
    })
    .expect("Dataflow execution failed");

    // Verify results
    let processed_results = results.lock().unwrap();
    assert_eq!(processed_results.len(), 2, "Should process 2 records");

    // Flush RDF store to ensure all writes complete
    rdf_store.flush_all().unwrap();

    // Verify RDF files were created
    let customers_file = temp_dir.path().join("customers.ttl");
    let products_file = temp_dir.path().join("products.ttl");

    assert!(customers_file.exists(), "customers.ttl should be created");
    assert!(products_file.exists(), "products.ttl should be created");

    // Read and verify customer RDF content
    let customers_rdf = std::fs::read_to_string(&customers_file).unwrap();

    println!("=== customers.ttl content ===");
    println!("{}", customers_rdf);

    // Verify customer record triples
    assert!(
        customers_rdf.contains("urn:graphica:record:customers/customer-001"),
        "Should contain customer record URN"
    );
    assert!(
        customers_rdf.contains("http://graphica.io/ontology#Record"),
        "Should contain Record type"
    );
    assert!(
        customers_rdf.contains("http://graphica.io/ontology#Field"),
        "Should contain Field type"
    );
    assert!(
        customers_rdf.contains("Email"),
        "Should contain Email semantic type"
    );
    assert!(
        customers_rdf.contains("PhoneNumber"),
        "Should contain PhoneNumber semantic type"
    );

    // Read and verify products RDF content
    let products_rdf = std::fs::read_to_string(&products_file).unwrap();

    println!("=== products.ttl content ===");
    println!("{}", products_rdf);

    // Verify product record triples
    assert!(
        products_rdf.contains("urn:graphica:record:products/product-001"),
        "Should contain product record URN"
    );
    assert!(
        products_rdf.contains("ProductCode"),
        "Should contain ProductCode semantic type (from SKU field)"
    );

    // Verify statistics
    let stats = rdf_store.get_statistics();
    assert_eq!(
        stats.total_records_persisted, 2,
        "Should have persisted 2 records"
    );
    assert_eq!(stats.active_datasets, 2, "Should have 2 active datasets");

    println!("✅ RDF persistence test passed!");
    println!("   - {} records persisted", stats.total_records_persisted);
    println!("   - {} datasets created", stats.active_datasets);
    println!("   - RDF files: customers.ttl, products.ttl");
}

#[test]
fn test_rdf_persistence_with_empty_metadata() {
    // Test that records without semantic metadata still create base triples
    let temp_dir = TempDir::new().unwrap();
    let rdf_config = RdfStoreConfig {
        base_dir: temp_dir.path().to_path_buf(),
        separate_files: true,
        flush_frequency: 1,
    };

    let dedup_state = Arc::new(CheckpointableDedupState::new(60_000, 100_000));
    let rdf_store = Arc::new(RdfStore::new(rdf_config, "test-source").unwrap());

    let rdf_clone = rdf_store.clone();

    timely::execute(Config::thread(), move |worker| {
        let dedup = (*dedup_state).clone();
        let store = rdf_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, _output, _resolved_entity_stream) = build_graphica_flow(
                scope,
                dedup.clone(),
                None, // No semantic detector
                Some(store.clone()),
                None, // No resolved entity config
            );

            // Record without semantic enrichment
            let record = Record {
                id: "test-001".to_string(),
                dataset: "test_dataset".to_string(),
                data: json!({"field1": "value1"}),
                source: DataRef {
                    system: "test".to_string(),
                    path: "test".to_string(),
                    version: None,
                    extracted_at: chrono::Utc::now(),
                    cdc_position: None,
                },
                timestamp: 1000,
                tenant_id: "test".to_string(),
                semantic_metadata: None, // No metadata
            };

            input.send(record);
            input.advance_to(1);
        });
    })
    .expect("Dataflow execution failed");

    // Flush and verify no file was created (no semantic metadata = no RDF persistence)
    rdf_store.flush_all().unwrap();

    let test_file = temp_dir.path().join("test_dataset.ttl");
    assert!(
        !test_file.exists(),
        "Should not create RDF file when no semantic metadata"
    );

    println!("✅ Empty metadata test passed - no RDF file created");
}
