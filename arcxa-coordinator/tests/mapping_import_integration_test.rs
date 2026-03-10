//! Integration tests for Phase 2: Data Import using Approved Mappings
//!
//! Tests the execute_import functionality directly

use graphica_coordinator::governance::rdf_store::GraphicaRdfStore;
use graphica_coordinator::mapping::{types::*, MappingEngine};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create a test mapping engine
async fn create_test_engine() -> (MappingEngine, TempDir, Arc<GraphicaRdfStore>) {
    let temp_dir = TempDir::new().unwrap();
    let rdf_store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let engine = MappingEngine::new(
        temp_dir.path().to_str().unwrap(),
        rdf_store.clone(), // PRE-EXISTING ISSUE: semantic_matcher parameter removed
    )
    .await
    .unwrap();
    (engine, temp_dir, rdf_store)
}

/// Create a basic mapping session in Active status for testing imports
async fn create_test_session_for_import(
    engine: &MappingEngine,
    source_id: &str,
) -> anyhow::Result<String> {
    use chrono::Utc;

    let session_id = format!("test_session_{}", uuid::Uuid::new_v4());

    // Create a test session with a mapped field
    let session = MappingSession {
        session_id: session_id.clone(),
        source_id: source_id.to_string(),
        status: MappingSessionStatus::Active, // Already active for import
        tables: vec![TableMapping {
            table_name: "customers".to_string(),
            field_mappings: vec![FieldMappingState {
                field_id: "field_001".to_string(),
                field_name: "customer_email".to_string(),
                data_type: "VARCHAR".to_string(),
                sample_values: vec!["test@example.com".to_string()],
                candidates: vec![],
                selected_mapping: Some(SelectedMapping {
                    ontology_term_uri: "http://schema.org/email".to_string(),
                    confidence: 0.95,
                    was_top_candidate: true,
                    transformation: None,
                }),
                approval_status: FieldApprovalStatus::Approved,
                reviewed_by: Some("test_user".to_string()),
                reviewed_at: Some(Utc::now().timestamp()),
                notes: None,
            }],
            metadata: None,
        }],
        created_by: "test_user".to_string(),
        created_at: Utc::now().timestamp(),
        reviewed_by: Some("test_user".to_string()),
        reviewed_at: Some(Utc::now().timestamp()),
        applied_at: Some(Utc::now().timestamp()),
        config: MappingSessionConfig::default(),
        summary: MappingSessionSummary {
            total_fields: 1,
            fields_with_candidates: 1,
            auto_approved: 0,
            pending_review: 0,
            user_approved: 1,
            rejected: 0,
            modified: 0,
            transformations_executed: 0,
            fields_used_in_transformations: 0,
            successful_transformations: 0,
            failed_transformations: 0,
        },
    };

    // Store session
    engine.storage.store_session(&session)?;

    Ok(session_id)
}

#[tokio::test]
async fn test_execute_import_with_demo_data() {
    println!("\n=== Test: Execute Import with Demo Data ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    // Create a test session in Active status
    let session_id = create_test_session_for_import(&engine, "test_source")
        .await
        .expect("Failed to create test session");

    println!("Created test session: {}", session_id);

    // Execute import
    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: None, // Use default
        tables: Some(vec!["customers".to_string()]),
        limit: Some(3), // Import 3 demo records
        user_id: "test_user".to_string(),
    };

    let import_response = engine
        .execute_import(&session_id, import_request)
        .await
        .expect("Import failed");

    // Verify response
    assert_eq!(import_response.status, ImportStatus::Completed);
    assert_eq!(import_response.stats.entities_created, 3);
    assert!(import_response.stats.triples_stored > 0);
    assert_eq!(import_response.stats.tables_imported, 1);
    assert_eq!(import_response.stats.errors.len(), 0);

    println!("✓ Import completed successfully");
    println!("  Import ID: {}", import_response.import_id);
    println!("  Status: {:?}", import_response.status);
    println!(
        "  Entities created: {}",
        import_response.stats.entities_created
    );
    println!("  Triples stored: {}", import_response.stats.triples_stored);
    println!(
        "  Processing time: {}ms",
        import_response.processing_time_ms
    );
    println!("  Target graph: {}", import_response.target_graph);

    println!("\n=== ✅ Test PASSED ===\n");
}

#[tokio::test]
async fn test_import_error_non_existent_session() {
    println!("\n=== Test: Import with Non-Existent Session ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: None,
        tables: None,
        limit: None,
        user_id: "test_user".to_string(),
    };

    let result = engine
        .execute_import("nonexistent_session", import_request)
        .await;

    assert!(result.is_err(), "Should fail with non-existent session");
    println!("✓ Correctly rejected non-existent session");

    println!("\n=== ✅ Test PASSED ===\n");
}

#[tokio::test]
async fn test_import_error_wrong_status() {
    println!("\n=== Test: Import with Wrong Session Status ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    // Create a session in Draft status
    let session_id = format!("test_session_{}", uuid::Uuid::new_v4());
    let session = MappingSession {
        session_id: session_id.clone(),
        source_id: "test_source".to_string(),
        status: MappingSessionStatus::Draft, // Not Active!
        tables: vec![],
        created_by: "test_user".to_string(),
        created_at: chrono::Utc::now().timestamp(),
        reviewed_by: None,
        reviewed_at: None,
        applied_at: None,
        config: MappingSessionConfig::default(),
        summary: MappingSessionSummary::default(),
    };

    engine
        .storage
        .store_session(&session)
        .expect("Failed to save session");

    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: None,
        tables: None,
        limit: None,
        user_id: "test_user".to_string(),
    };

    let result = engine.execute_import(&session_id, import_request).await;

    assert!(result.is_err(), "Should fail with Draft session");
    println!("✓ Correctly rejected Draft session");

    println!("\n=== ✅ Test PASSED ===\n");
}

#[tokio::test]
async fn test_import_custom_target_graph() {
    println!("\n=== Test: Import to Custom Named Graph ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    let session_id = create_test_session_for_import(&engine, "custom_source")
        .await
        .expect("Failed to create test session");

    let custom_graph = "http://graphica.io/graph/custom/test".to_string();

    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: Some(custom_graph.clone()),
        tables: Some(vec!["customers".to_string()]),
        limit: Some(3),
        user_id: "test_user".to_string(),
    };

    let import_response = engine
        .execute_import(&session_id, import_request)
        .await
        .expect("Import failed");

    assert_eq!(import_response.target_graph, custom_graph);
    assert_eq!(import_response.status, ImportStatus::Completed);

    println!(
        "✓ Imported to custom graph: {}",
        import_response.target_graph
    );

    println!("\n=== ✅ Test PASSED ===\n");
}

#[tokio::test]
async fn test_import_statistics_accuracy() {
    println!("\n=== Test: Import Statistics Accuracy ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    let session_id = create_test_session_for_import(&engine, "stats_test_source")
        .await
        .expect("Failed to create test session");

    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: None,
        tables: Some(vec!["customers".to_string()]),
        limit: Some(5), // Import 5 records
        user_id: "test_user".to_string(),
    };

    let import_response = engine
        .execute_import(&session_id, import_request)
        .await
        .expect("Import failed");

    let stats = &import_response.stats;

    println!("Import Statistics:");
    println!("  Rows processed: {}", stats.rows_processed);
    println!("  Entities created: {}", stats.entities_created);
    println!("  Triples stored: {}", stats.triples_stored);
    println!("  Tables imported: {}", stats.tables_imported);
    println!("  Fields mapped: {}", stats.fields_mapped);
    println!("  Errors: {}", stats.errors.len());

    // Verify statistics match expectations
    assert_eq!(stats.rows_processed, 5, "Should process 5 rows");
    assert_eq!(stats.entities_created, 5, "Should create 5 entities");
    assert!(stats.triples_stored > 0, "Should store triples");
    assert_eq!(stats.tables_imported, 1, "Should import 1 table");
    assert!(stats.fields_mapped > 0, "Should have mapped fields");
    assert_eq!(stats.errors.len(), 0, "Should have no errors");

    println!("✓ All statistics match expected values");

    println!("\n=== ✅ Test PASSED ===\n");
}

#[tokio::test]
async fn test_import_processing_time() {
    println!("\n=== Test: Import Processing Time ===\n");

    let (engine, _temp_dir, _rdf_store) = create_test_engine().await;

    let session_id = create_test_session_for_import(&engine, "perf_test_source")
        .await
        .expect("Failed to create test session");

    let import_request = ImportDataRequest {
        batch_size: 100,
        target_graph: None,
        tables: Some(vec!["customers".to_string()]),
        limit: Some(3),
        user_id: "test_user".to_string(),
    };

    let import_response = engine
        .execute_import(&session_id, import_request)
        .await
        .expect("Import failed");

    assert!(
        import_response.processing_time_ms > 0,
        "Should report processing time"
    );
    assert!(
        import_response.processing_time_ms < 5000,
        "Should complete within 5 seconds"
    );

    println!(
        "✓ Import completed in {}ms",
        import_response.processing_time_ms
    );

    println!("\n=== ✅ Test PASSED ===\n");
}
