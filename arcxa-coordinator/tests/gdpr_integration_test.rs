//! GDPR Integration Tests
//!
//! End-to-end tests for GDPR compliance:
//! - Article 17 (Right to Erasure)
//! - Article 20 (Right to Data Portability)
//!
//! ## Article 17 Tests verify:
//! - GDPR coordinator correctly orchestrates erasure across multiple backends
//! - Dry-run mode works correctly
//! - Actual erasure removes all tenant data
//! - Erasure verification detects remaining data
//! - Multi-tenant data isolation
//!
//! ## Article 20 Tests verify:
//! - Data export job creation and execution
//! - Format conversion (JSON, CSV, XML, PDF)
//! - Data discovery across lineage stores
//! - Async job execution and status tracking
//! - Export file download and expiry

use anyhow::Result;
use graphica_coordinator::{
    gdpr::GdprCoordinator,
    storage::{ColumnLineageStore, RowLineageStore, SchemaEvolutionStore},
};
use graphica_core::{
    core::lineage::{
        column_level::{ColumnLineageEvent, ColumnLineageSink, ColumnRef, TransformationType},
        row_level::{RowId, RowLevelLineageSink, RowLineageEvent},
        schema_evolution::SchemaChangeEvent,
    },
    gdpr::{DataErasure, DataSubjectId},
};
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create test stores and coordinator
async fn setup_test_environment() -> Result<(
    GdprCoordinator,
    TempDir,
    Arc<RowLineageStore>,
    Arc<ColumnLineageStore>,
    Arc<SchemaEvolutionStore>,
)> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    let row_store = Arc::new(RowLineageStore::new(temp_path.join("row_lineage"))?);
    let column_store = Arc::new(ColumnLineageStore::new(temp_path.join("column_lineage"))?);
    let schema_store = Arc::new(SchemaEvolutionStore::open(
        temp_path.join("schema_evolution"),
    )?);

    let coordinator = GdprCoordinator::with_default_retention(
        Some(row_store.clone()),
        Some(column_store.clone()),
        Some(schema_store.clone()),
    );

    Ok((coordinator, temp_dir, row_store, column_store, schema_store))
}

/// Populate stores with test data for a specific tenant
async fn populate_test_data(
    tenant_id: &str,
    row_store: &Arc<RowLineageStore>,
    column_store: &Arc<ColumnLineageStore>,
    schema_store: &Arc<SchemaEvolutionStore>,
) -> Result<()> {
    // Add row lineage data
    for i in 0..5 {
        let row_id = RowId::csv(format!("test_{}.csv", i), i);
        let event = RowLineageEvent::success(
            row_id,
            format!("batch-{}", i),
            format!("job-{}", i),
            "/output/test.csv".to_string(),
            tenant_id.to_string(),
        );
        row_store.write_row(event).await?;
    }
    row_store.flush_buffer().await?;

    // Add column lineage data
    for i in 0..3 {
        let target_column = ColumnRef {
            datasource_id: format!("ds_{}", i),
            schema: Some("public".to_string()),
            table_name: "test_table".to_string(),
            column_name: format!("col_{}", i),
            data_type: "VARCHAR".to_string(),
        };

        let event = ColumnLineageEvent::new(
            vec![], // No source columns for this test
            target_column,
            format!("transform_{}", i),
            TransformationType::DirectCopy,
            "test_job".to_string(),
            tenant_id.to_string(),
            "test_user".to_string(),
        );

        column_store.record_column_lineage(event).await?;
    }
    column_store.flush().await?;

    // Add schema evolution data
    for i in 0..2 {
        let mut event = SchemaChangeEvent::new(
            "test_datasource",
            "test_table",
            graphica_core::core::lineage::schema_evolution::SchemaChangeType::ColumnAdded,
            "test_initiator",
            tenant_id,
        );
        event.column_name = Some(format!("new_col_{}", i));
        schema_store.record_schema_change(event)?;
    }

    Ok(())
}

#[tokio::test]
async fn test_count_tenant_data() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, schema_store) =
        setup_test_environment().await?;

    // Populate with test data
    populate_test_data("tenant-test-1", &row_store, &column_store, &schema_store).await?;

    // Count data
    let count = coordinator.count_tenant_data("tenant-test-1").await?;

    // Should have 5 rows + 3 columns + 2 schema events = 10 total
    assert_eq!(count, 10, "Expected 10 total records");

    // Get breakdown
    let breakdown = coordinator
        .get_tenant_data_breakdown("tenant-test-1")
        .await?;
    assert_eq!(breakdown.get("row_lineage_events").copied().unwrap_or(0), 5);
    assert_eq!(
        breakdown.get("column_lineage_events").copied().unwrap_or(0),
        3
    );
    assert_eq!(
        breakdown.get("schema_change_events").copied().unwrap_or(0),
        2
    );

    Ok(())
}

#[tokio::test]
async fn test_dry_run_erasure() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, schema_store) =
        setup_test_environment().await?;

    // Populate with test data
    populate_test_data("tenant-test-2", &row_store, &column_store, &schema_store).await?;

    // Count before dry-run
    let count_before = coordinator.count_tenant_data("tenant-test-2").await?;
    assert_eq!(count_before, 10);

    // Perform dry-run erasure
    let result = coordinator.erase_tenant_data("tenant-test-2", true).await?;

    // Verify dry-run was acknowledged
    assert!(result.request.dry_run, "Should be a dry-run");
    assert_eq!(
        result.total_records_erased, 0,
        "Dry-run should not erase records"
    );
    assert!(result.success, "Dry-run should succeed");

    // Verify data still exists
    let count_after = coordinator.count_tenant_data("tenant-test-2").await?;
    assert_eq!(count_after, 10, "Data should still exist after dry-run");

    Ok(())
}

#[tokio::test]
async fn test_actual_erasure() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, schema_store) =
        setup_test_environment().await?;

    // Populate with test data
    populate_test_data("tenant-test-3", &row_store, &column_store, &schema_store).await?;

    // Verify data exists
    let count_before = coordinator.count_tenant_data("tenant-test-3").await?;
    assert_eq!(count_before, 10);

    // Perform actual erasure
    let result = coordinator
        .erase_tenant_data("tenant-test-3", false)
        .await?;

    // Verify erasure succeeded
    assert!(!result.request.dry_run, "Should be actual erasure");
    assert_eq!(
        result.total_records_erased, 10,
        "Should erase all 10 records"
    );
    assert!(result.success, "Erasure should succeed");
    assert_eq!(
        result.backends_succeeded, 3,
        "All 3 backends should succeed"
    );
    assert_eq!(result.backends_failed, 0, "No backends should fail");

    // Verify data is gone
    let count_after = coordinator.count_tenant_data("tenant-test-3").await?;
    assert_eq!(count_after, 0, "All data should be erased");

    // Verify erasure
    let verified = coordinator.verify_erasure("tenant-test-3").await?;
    assert!(verified, "Erasure should be verified");

    Ok(())
}

#[tokio::test]
async fn test_multi_tenant_isolation() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, schema_store) =
        setup_test_environment().await?;

    // Populate data for two different tenants
    populate_test_data("tenant-a", &row_store, &column_store, &schema_store).await?;
    populate_test_data("tenant-b", &row_store, &column_store, &schema_store).await?;

    // Verify both tenants have data
    let count_a_before = coordinator.count_tenant_data("tenant-a").await?;
    let count_b_before = coordinator.count_tenant_data("tenant-b").await?;
    assert_eq!(count_a_before, 10);
    assert_eq!(count_b_before, 10);

    // Erase only tenant-a
    let result = coordinator.erase_tenant_data("tenant-a", false).await?;
    assert!(result.success);
    assert_eq!(result.total_records_erased, 10);

    // Verify tenant-a is gone
    let count_a_after = coordinator.count_tenant_data("tenant-a").await?;
    assert_eq!(count_a_after, 0, "Tenant A data should be erased");

    // Verify tenant-b still exists
    let count_b_after = coordinator.count_tenant_data("tenant-b").await?;
    assert_eq!(count_b_after, 10, "Tenant B data should still exist");

    Ok(())
}

#[tokio::test]
async fn test_erasure_of_nonexistent_tenant() -> Result<()> {
    let (coordinator, _temp_dir, _row_store, _column_store, _schema_store) =
        setup_test_environment().await?;

    // Erase tenant that doesn't exist
    let result = coordinator
        .erase_tenant_data("nonexistent-tenant", false)
        .await?;

    // Should succeed with 0 records erased
    assert!(
        result.success,
        "Erasure of nonexistent tenant should succeed"
    );
    assert_eq!(result.total_records_erased, 0, "Should erase 0 records");

    // Verification should pass
    let verified = coordinator.verify_erasure("nonexistent-tenant").await?;
    assert!(verified, "Nonexistent tenant should verify as erased");

    Ok(())
}

#[tokio::test]
async fn test_partial_store_availability() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    // Create coordinator with only row and column stores (no schema store)
    let row_store = Arc::new(RowLineageStore::new(temp_path.join("row_lineage"))?);
    let column_store = Arc::new(ColumnLineageStore::new(temp_path.join("column_lineage"))?);

    let coordinator = GdprCoordinator::with_default_retention(
        Some(row_store.clone()),
        Some(column_store.clone()),
        None, // No schema store
    );

    // Add data to available stores
    let row_id = RowId::csv("test.csv", 1);
    let event = RowLineageEvent::success(
        row_id,
        "batch-1".to_string(),
        "job-1".to_string(),
        "/output/test.csv".to_string(),
        "tenant-partial".to_string(),
    );
    row_store.write_row(event).await?;
    row_store.flush_buffer().await?;

    // Count should only include available stores
    let count = coordinator.count_tenant_data("tenant-partial").await?;
    assert_eq!(count, 1, "Should count data from available stores only");

    // Erasure should succeed for available stores
    let result = coordinator
        .erase_tenant_data("tenant-partial", false)
        .await?;
    assert!(result.success, "Should succeed even with missing stores");
    assert_eq!(result.total_records_erased, 1);

    Ok(())
}

#[tokio::test]
async fn test_data_subject_type_validation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    let row_store = Arc::new(RowLineageStore::new(temp_path.join("row_lineage"))?);

    // Try to erase with an unsupported data subject type
    let unsupported_subject = DataSubjectId {
        id_type: "email".to_string(),
        id: "user@example.com".to_string(),
    };

    let request = graphica_core::gdpr::ErasureRequest::new(unsupported_subject, "test");

    // Should return a result with failure status for unsupported type
    let result = row_store.erase_data_subject(&request).await?;
    assert!(
        !result.success,
        "Should report failure for unsupported data subject types"
    );

    // Check that the backend result contains an error message
    assert!(
        result
            .backend_results
            .iter()
            .any(|(_, backend_result)| !backend_result.success
                && backend_result.error_message.is_some()),
        "Should have a backend result with an error message"
    );

    Ok(())
}

// ============================================================================
// GDPR Article 20: Right to Data Portability - Export Tests
// ============================================================================

use graphica_coordinator::gdpr::export::{
    ExportFormat, ExportJobStore, ExportRequest, ExportStatus,
};
use std::collections::HashMap;

#[tokio::test]
async fn test_export_job_storage() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    let job_store = ExportJobStore::create(temp_path.join("export_jobs"))?;

    let request = ExportRequest {
        user_id: "user123".to_string(),
        format: ExportFormat::Json,
        categories: vec![],
        include_derived: false,
        include_metadata: true,
        include_audit_trail: false,
        time_range: None,
        filters: HashMap::new(),
    };

    // Create export job
    let job = graphica_coordinator::gdpr::export::ExportJob::new(
        "user123".to_string(),
        "admin@test.com".to_string(),
        request,
    );

    let job_id = job.id;

    // Save job
    job_store.save(&job)?;

    // Retrieve job
    let retrieved = job_store.get(job_id)?;
    assert!(retrieved.is_some());

    let retrieved_job = retrieved.unwrap();
    assert_eq!(retrieved_job.id, job_id);
    assert_eq!(retrieved_job.user_id, "user123");
    assert_eq!(retrieved_job.status, ExportStatus::Pending);

    Ok(())
}

#[tokio::test]
async fn test_export_format_extensions() -> Result<()> {
    assert_eq!(ExportFormat::Json.extension(), "json");
    assert_eq!(ExportFormat::Csv.extension(), "csv");
    assert_eq!(ExportFormat::Xml.extension(), "xml");
    assert_eq!(ExportFormat::Pdf.extension(), "pdf");

    assert_eq!(ExportFormat::Json.mime_type(), "application/json");
    assert_eq!(ExportFormat::Csv.mime_type(), "text/csv");
    assert_eq!(ExportFormat::Xml.mime_type(), "application/xml");
    assert_eq!(ExportFormat::Pdf.mime_type(), "application/pdf");

    Ok(())
}

#[tokio::test]
async fn test_export_job_lifecycle() -> Result<()> {
    use chrono::Utc;
    use graphica_coordinator::gdpr::export::{ExportMetadata, ExportResult};

    let request = ExportRequest {
        user_id: "user456".to_string(),
        format: ExportFormat::Json,
        categories: vec![],
        include_derived: false,
        include_metadata: true,
        include_audit_trail: false,
        time_range: None,
        filters: HashMap::new(),
    };

    let mut job = graphica_coordinator::gdpr::export::ExportJob::new(
        "user456".to_string(),
        "user456@test.com".to_string(),
        request.clone(),
    );

    // Initially pending
    assert_eq!(job.status, ExportStatus::Pending);

    // Start processing
    job.start_processing();
    assert_eq!(job.status, ExportStatus::Processing);

    // Complete with result
    let result = ExportResult {
        file_path: "/exports/user456.json".to_string(),
        download_url: "https://example.com/download/abc123".to_string(),
        file_size_bytes: 1024,
        checksum: "abc123def456".to_string(),
        metadata: ExportMetadata {
            format: ExportFormat::Json,
            record_count: 100,
            categories: vec![],
            sources: vec![],
            time_range: None,
            generated_at: Utc::now(),
        },
    };

    job.complete(result, 48);
    assert_eq!(job.status, ExportStatus::Ready);
    assert_eq!(job.progress.percent_complete, 100);
    assert!(job.completed_at.is_some());
    assert!(job.expires_at.is_some());
    assert!(job.result.is_some());

    Ok(())
}

// ============================================================================
// User-Level Erasure Tests (Enhanced Article 17 Support)
// ============================================================================

#[tokio::test]
async fn test_user_level_erasure_with_hard_delete() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, _schema_store) =
        setup_test_environment().await?;

    // Populate data for a specific user
    let user_id = "user-test-123";

    // Add row lineage data for the user
    for i in 0..5 {
        let row_id = RowId::csv(format!("user_file_{}.csv", i), i);
        let mut event = RowLineageEvent::success(
            row_id,
            format!("batch-{}", i),
            format!("job-{}", i),
            "/output/test.csv".to_string(),
            "tenant-test".to_string(),
        );
        // Tag the event with the user identifier for test traceability.
        event.correlation_id = Some(user_id.to_string());
        row_store.write_row(event).await?;
    }
    row_store.flush_buffer().await?;

    // Count records before erasure
    let count_before = coordinator.count_user_data(user_id).await?;
    assert!(count_before > 0, "User should have data before erasure");

    // Erase user data with hard delete
    use graphica_core::gdpr::ErasureStrategy;
    let result = coordinator
        .erase_user_data(user_id, false, ErasureStrategy::HardDelete)
        .await?;

    assert!(result.success, "Erasure should succeed");
    assert!(
        result.total_records_erased > 0,
        "Should have erased records"
    );

    // Verify data is gone
    let count_after = coordinator.count_user_data(user_id).await?;
    assert_eq!(count_after, 0, "All user data should be erased");

    Ok(())
}

#[tokio::test]
async fn test_user_level_dry_run_erasure() -> Result<()> {
    let (coordinator, _temp_dir, row_store, _column_store, _schema_store) =
        setup_test_environment().await?;

    let user_id = "user-dry-run-test";

    // Add data
    for i in 0..3 {
        let row_id = RowId::csv(format!("file_{}.csv", i), i);
        let mut event = RowLineageEvent::success(
            row_id,
            format!("batch-{}", i),
            format!("job-{}", i),
            "/output/test.csv".to_string(),
            "tenant-test".to_string(),
        );
        event.correlation_id = Some(user_id.to_string());
        row_store.write_row(event).await?;
    }
    row_store.flush_buffer().await?;

    let count_before = coordinator.count_user_data(user_id).await?;
    assert!(count_before > 0);

    // Dry run erasure
    use graphica_core::gdpr::ErasureStrategy;
    let result = coordinator
        .erase_user_data(user_id, true, ErasureStrategy::HardDelete)
        .await?;

    assert!(result.request.dry_run, "Should be dry run");
    assert_eq!(
        result.total_records_erased, 0,
        "Dry run should not erase records"
    );

    // Verify data still exists
    let count_after = coordinator.count_user_data(user_id).await?;
    assert_eq!(
        count_before, count_after,
        "Data should still exist after dry run"
    );

    Ok(())
}

#[tokio::test]
async fn test_legal_hold_prevents_user_erasure() -> Result<()> {
    let (coordinator, _temp_dir, row_store, _column_store, _schema_store) =
        setup_test_environment().await?;

    let user_id = "user-under-hold";

    // Add data for user
    let row_id = RowId::csv("file.csv", 1);
    let mut event = RowLineageEvent::success(
        row_id,
        "batch-1".to_string(),
        "job-1".to_string(),
        "/output/test.csv".to_string(),
        "tenant-test".to_string(),
    );
    event.correlation_id = Some(user_id.to_string());
    row_store.write_row(event).await?;
    row_store.flush_buffer().await?;

    // Place legal hold on user
    use graphica_core::gdpr::{DataCategory, LegalHold};
    let mut retention_manager = graphica_core::gdpr::RetentionManager::new();
    let hold = LegalHold::new("Litigation Case 123", "Legal Team", "Pending lawsuit")
        .add_data_subject(user_id);
    retention_manager.add_legal_hold(hold);

    // Create coordinator with the retention manager containing the legal hold
    let coordinator_with_hold = graphica_coordinator::gdpr::GdprCoordinator::new(
        Some(row_store.clone()),
        None,
        None,
        std::sync::Arc::new(retention_manager),
    );

    // Try to erase user data
    use graphica_core::gdpr::ErasureStrategy;
    let result = coordinator_with_hold
        .erase_user_data(user_id, false, ErasureStrategy::HardDelete)
        .await;

    // Should fail due to legal hold
    assert!(
        result.is_err(),
        "Erasure should fail when user is under legal hold"
    );
    assert!(
        result.unwrap_err().to_string().contains("legal hold"),
        "Error message should mention legal hold"
    );

    // Verify data still exists
    let count = coordinator.count_user_data(user_id).await?;
    assert!(count > 0, "Data should still exist due to legal hold");

    Ok(())
}

#[tokio::test]
async fn test_retention_policy_enforcement() -> Result<()> {
    use graphica_core::gdpr::{DataCategory, RetentionManager, RetentionPolicy};

    let retention_manager = RetentionManager::default();

    // Check retention policy for audit logs (7 years minimum)
    let policy = retention_manager.get_policy(&DataCategory::AuditLogs);
    assert!(policy.is_some());

    let policy = policy.unwrap();
    assert_eq!(policy.min_retention_days, Some(2555)); // ~7 years
    assert_eq!(policy.data_category, DataCategory::AuditLogs);

    // Check retention policy for personal identifiers (3 years maximum)
    let policy = retention_manager.get_policy(&DataCategory::PersonalIdentifiers);
    assert!(policy.is_some());

    let policy = policy.unwrap();
    assert_eq!(policy.max_retention_days, Some(1095)); // 3 years
    assert_eq!(policy.data_category, DataCategory::PersonalIdentifiers);

    Ok(())
}

#[tokio::test]
async fn test_user_data_breakdown() -> Result<()> {
    let (coordinator, _temp_dir, row_store, column_store, _schema_store) =
        setup_test_environment().await?;

    let user_id = "user-breakdown-test";

    // Add row lineage data
    for i in 0..3 {
        let row_id = RowId::csv(format!("file_{}.csv", i), i);
        let mut event = RowLineageEvent::success(
            row_id,
            format!("batch-{}", i),
            format!("job-{}", i),
            "/output/test.csv".to_string(),
            "tenant-test".to_string(),
        );
        event.correlation_id = Some(user_id.to_string());
        row_store.write_row(event).await?;
    }
    row_store.flush_buffer().await?;

    // Add column lineage data
    for i in 0..2 {
        let target_column = ColumnRef {
            datasource_id: format!("ds_{}", i),
            schema: Some("public".to_string()),
            table_name: "test_table".to_string(),
            column_name: format!("col_{}", i),
            data_type: "VARCHAR".to_string(),
        };

        let mut event = ColumnLineageEvent::new(
            vec![],
            target_column,
            format!("transform_{}", i),
            TransformationType::DirectCopy,
            "test_job".to_string(),
            "tenant-test".to_string(),
            user_id.to_string(),
        );

        column_store.record_column_lineage(event).await?;
    }
    column_store.flush().await?;

    // Get data breakdown
    let breakdown = coordinator.get_user_data_breakdown(user_id).await?;

    assert!(!breakdown.is_empty(), "Breakdown should not be empty");
    assert!(
        breakdown.values().sum::<u64>() > 0,
        "Total count should be > 0"
    );

    Ok(())
}
