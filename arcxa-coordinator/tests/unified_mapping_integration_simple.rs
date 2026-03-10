//! Simple Integration Test for CSV-to-DB Unified Mapping Workflow
//!
//! This test validates the basic end-to-end workflow:
//! 1. Create source mapping sessions
//! 2. Create unified session
//! 3. Generate target database DDL
//! 4. Query lineage

use anyhow::Result;
use graphica_coordinator::mapping::lineage::ExtendedLineageService;
use graphica_coordinator::mapping::{
    loader::PostgreSQLLoader,
    multi_source::{
        // Fixed: Was incorrectly importing from non-existent 'unified' module
        CreateUnifiedSessionRequest,
        TargetColumnConfig,
        TargetDatabaseConfig,
        TargetTableConfig,
        UnifiedMappingCoordinator,
        UnifiedMappingStorage,
        UnifiedSessionStatus,
    },
    storage::MappingStorage,
    types::{MappingSessionConfig, MappingSessionStatus, MappingSessionSummary},
    FieldApprovalStatus, FieldMappingState, MappingSession, SelectedMapping, TableMapping,
};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_simple_end_to_end_workflow() -> Result<()> {
    // Setup storage
    let source_dir = TempDir::new()?;
    let unified_dir = TempDir::new()?;

    let source_storage = Arc::new(MappingStorage::new(source_dir.path().to_str().unwrap())?);
    let unified_storage = Arc::new(UnifiedMappingStorage::new(
        unified_dir.path().to_str().unwrap(),
    )?);

    // Step 1: Create source mapping session
    let source_session = MappingSession {
        session_id: "csv1_session".to_string(),
        source_id: "csv1".to_string(),
        status: MappingSessionStatus::Active,
        tables: vec![TableMapping {
            table_name: "data".to_string(),
            field_mappings: vec![FieldMappingState {
                field_id: "csv1_email".to_string(),
                field_name: "email".to_string(),
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
                reviewed_at: Some(1000000),
                notes: None,
            }],
            metadata: None,
        }],
        created_by: "test_user".to_string(),
        created_at: 1000000,
        reviewed_by: None,
        reviewed_at: None,
        applied_at: None,
        config: MappingSessionConfig::default(),
        summary: MappingSessionSummary::default(),
    };

    source_storage.store_session(&source_session)?;

    // Step 2: Create unified session
    let coordinator =
        UnifiedMappingCoordinator::new(source_storage.clone(), unified_storage.clone());

    let target_database = TargetDatabaseConfig {
        datasource_id: "postgres_prod".to_string(),
        schema: "public".to_string(),
        tables: HashMap::new(),
    };

    let request = CreateUnifiedSessionRequest {
        source_session_ids: vec!["csv1_session".to_string()],
        target_database,
        created_by: "test_user".to_string(),
    };

    let response = coordinator.create_unified_session(request).await?;

    // Verify response
    assert!(response.session_id.starts_with("unified_"));
    assert_eq!(response.field_mappings_count, 1);
    assert_eq!(response.conflicts_detected, 0);
    assert!(matches!(response.status, UnifiedSessionStatus::ReadyToLoad));

    // Step 3: Retrieve unified session
    let unified_session = coordinator
        .get_unified_session(&response.session_id)?
        .expect("Unified session should exist");

    assert_eq!(unified_session.field_mappings.len(), 1);
    assert_eq!(unified_session.conflicts.len(), 0);

    // Step 4: Generate DDL
    let mut columns = HashMap::new();
    columns.insert(
        "email".to_string(),
        TargetColumnConfig {
            name: "email".to_string(),
            data_type: "VARCHAR(255)".to_string(),
            nullable: false,
            is_primary_key: false,
            default_value: None,
        },
    );

    let table_config = TargetTableConfig {
        name: "customers".to_string(),
        columns,
        primary_keys: vec![],
        foreign_keys: vec![],
    };

    let loader = PostgreSQLLoader::with_defaults();
    let ddl = loader.generate_create_table_ddl(&table_config)?;

    // Verify DDL
    assert!(ddl.contains("CREATE TABLE customers"));
    assert!(ddl.contains("email VARCHAR(255) NOT NULL"));

    // Step 5: Generate lineage SPARQL
    let lineage_service = ExtendedLineageService::new();
    let sparql = lineage_service.generate_lineage_sparql("csv1", "email");

    // Verify SPARQL
    assert!(sparql.contains("gph:FieldMapping"));
    assert!(sparql.contains("gph:UnifiedFieldMapping"));
    assert!(sparql.contains("csv1"));
    assert!(sparql.contains("email"));

    Ok(())
}

#[tokio::test]
async fn test_conflict_detection() -> Result<()> {
    // Setup storage
    let source_dir = TempDir::new()?;
    let unified_dir = TempDir::new()?;

    let source_storage = Arc::new(MappingStorage::new(source_dir.path().to_str().unwrap())?);
    let unified_storage = Arc::new(UnifiedMappingStorage::new(
        unified_dir.path().to_str().unwrap(),
    )?);

    // Create two sessions mapping to same ontology term
    let session1 = MappingSession {
        session_id: "csv1_session".to_string(),
        source_id: "csv1".to_string(),
        status: MappingSessionStatus::Active,
        tables: vec![TableMapping {
            table_name: "data".to_string(),
            field_mappings: vec![FieldMappingState {
                field_id: "csv1_email".to_string(),
                field_name: "email".to_string(),
                data_type: "VARCHAR".to_string(),
                sample_values: vec![],
                candidates: vec![],
                selected_mapping: Some(SelectedMapping {
                    ontology_term_uri: "http://schema.org/email".to_string(),
                    confidence: 0.95,
                    was_top_candidate: true,
                    transformation: None,
                }),
                approval_status: FieldApprovalStatus::Approved,
                reviewed_by: Some("test_user".to_string()),
                reviewed_at: Some(1000000),
                notes: None,
            }],
            metadata: None,
        }],
        created_by: "test_user".to_string(),
        created_at: 1000000,
        reviewed_by: None,
        reviewed_at: None,
        applied_at: None,
        config: MappingSessionConfig::default(),
        summary: MappingSessionSummary::default(),
    };

    let session2 = MappingSession {
        session_id: "csv2_session".to_string(),
        source_id: "csv2".to_string(),
        status: MappingSessionStatus::Active,
        tables: vec![TableMapping {
            table_name: "data".to_string(),
            field_mappings: vec![FieldMappingState {
                field_id: "csv2_email_address".to_string(),
                field_name: "email_address".to_string(),
                data_type: "VARCHAR".to_string(),
                sample_values: vec![],
                candidates: vec![],
                selected_mapping: Some(SelectedMapping {
                    ontology_term_uri: "http://schema.org/email".to_string(), // Same term!
                    confidence: 0.90,
                    was_top_candidate: true,
                    transformation: None,
                }),
                approval_status: FieldApprovalStatus::Approved,
                reviewed_by: Some("test_user".to_string()),
                reviewed_at: Some(1000000),
                notes: None,
            }],
            metadata: None,
        }],
        created_by: "test_user".to_string(),
        created_at: 1000000,
        reviewed_by: None,
        reviewed_at: None,
        applied_at: None,
        config: MappingSessionConfig::default(),
        summary: MappingSessionSummary::default(),
    };

    source_storage.store_session(&session1)?;
    source_storage.store_session(&session2)?;

    // Create unified session
    let coordinator =
        UnifiedMappingCoordinator::new(source_storage.clone(), unified_storage.clone());

    let target_database = TargetDatabaseConfig {
        datasource_id: "postgres_prod".to_string(),
        schema: "public".to_string(),
        tables: HashMap::new(),
    };

    let request = CreateUnifiedSessionRequest {
        source_session_ids: vec!["csv1_session".to_string(), "csv2_session".to_string()],
        target_database,
        created_by: "test_user".to_string(),
    };

    let response = coordinator.create_unified_session(request).await?;

    // Verify conflict was detected
    assert_eq!(response.conflicts_detected, 1);
    assert!(matches!(
        response.status,
        UnifiedSessionStatus::ConflictsDetected
    ));

    // Retrieve and verify conflict details
    let unified_session = coordinator
        .get_unified_session(&response.session_id)?
        .expect("Unified session should exist");

    assert_eq!(unified_session.conflicts.len(), 1);
    let conflict = &unified_session.conflicts[0];
    assert_eq!(conflict.conflicting_sources.len(), 2);
    assert!(!conflict.resolved);

    Ok(())
}
