//! Integration tests for column-level lineage API
//!
//! Tests the complete flow: storage → API handlers → responses

use graphica_coordinator::storage::column_lineage_store::ColumnLineageStore;
use graphica_core::core::lineage::column_level::{
    ColumnImpactAnalysis, ColumnLineageEvent, ColumnLineageGraph, ColumnLineageSink, ColumnRef,
    TransformationType,
};
use tempfile::TempDir;

#[tokio::test]
async fn test_column_lineage_basic_flow() {
    // Create temporary database
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    // Create test columns
    let source_col = ColumnRef::new("db1", "customers", "email", "VARCHAR(255)");
    let target_col = ColumnRef::new("db2", "users", "user_email", "VARCHAR(255)");

    // Create lineage event
    let event = ColumnLineageEvent::new(
        vec![source_col.clone()],
        target_col.clone(),
        "user_email = LOWER(email)".to_string(),
        TransformationType::SqlExpression,
        "job-123".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    // Write lineage
    store.record_column_lineage(event.clone()).await.unwrap();
    store.flush().await.unwrap();

    // Read back lineage
    let events = store.get_column_lineage(&target_col).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].job_id, "job-123");
    assert_eq!(events[0].source_columns.len(), 1);
    assert_eq!(events[0].source_columns[0].column_name, "email");
}

#[tokio::test]
async fn test_column_lineage_graph_traversal() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    // Build a simple lineage chain: col1 -> col2 -> col3
    let col1 = ColumnRef::new("source", "raw_data", "value", "INT");
    let col2 = ColumnRef::new("staging", "cleaned_data", "clean_value", "INT");
    let col3 = ColumnRef::new("prod", "fact_sales", "amount", "INT");

    // Event 1: col1 -> col2
    let event1 = ColumnLineageEvent::new(
        vec![col1.clone()],
        col2.clone(),
        "clean_value = COALESCE(value, 0)".to_string(),
        TransformationType::SqlExpression,
        "job-1".to_string(),
        "tenant-1".to_string(),
        "etl".to_string(),
    );

    // Event 2: col2 -> col3
    let event2 = ColumnLineageEvent::new(
        vec![col2.clone()],
        col3.clone(),
        "amount = clean_value * 100".to_string(),
        TransformationType::MathOperation {
            operation: "multiply".to_string(),
        },
        "job-2".to_string(),
        "tenant-1".to_string(),
        "etl".to_string(),
    );

    // Write events
    store
        .record_column_lineage_batch(vec![event1, event2])
        .await
        .unwrap();

    // Trace graph from col3
    let graph = store.trace_column_graph(&col3, 10).await.unwrap();

    assert_eq!(graph.column, col3);
    assert_eq!(graph.source_columns.len(), 2); // col1 and col2
    assert_eq!(graph.lineage_depth, 2); // 2 hops
    assert_eq!(graph.total_transformations, 2);
    assert!(!graph.statistics.has_circular_dependency);
}

#[tokio::test]
async fn test_column_impact_analysis() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    // Create a column that feeds into multiple targets
    let source = ColumnRef::new("source", "raw_customers", "customer_id", "INT");
    let target1 = ColumnRef::new("prod", "dim_customer", "id", "INT");
    let target2 = ColumnRef::new("prod", "fact_orders", "customer_ref", "INT");

    let event1 = ColumnLineageEvent::new(
        vec![source.clone()],
        target1.clone(),
        "id = customer_id".to_string(),
        TransformationType::DirectCopy,
        "job-1".to_string(),
        "tenant-1".to_string(),
        "etl".to_string(),
    );

    let event2 = ColumnLineageEvent::new(
        vec![source.clone()],
        target2.clone(),
        "customer_ref = customer_id".to_string(),
        TransformationType::DirectCopy,
        "job-2".to_string(),
        "tenant-1".to_string(),
        "etl".to_string(),
    );

    store
        .record_column_lineage_batch(vec![event1, event2])
        .await
        .unwrap();

    // Analyze impact
    let impact = store.analyze_column_impact(&source).await.unwrap();

    assert_eq!(impact.source_column, source);
    assert_eq!(impact.affected_columns.len(), 2);
    assert_eq!(impact.affected_jobs.len(), 2);

    // Both targets are in "prod" datasource - should be critical
    assert_eq!(impact.critical_dependencies.len(), 2);
}

#[tokio::test]
async fn test_derived_columns() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    let source = ColumnRef::new("db1", "customers", "first_name", "VARCHAR(100)");
    let derived1 = ColumnRef::new("db2", "users", "fname", "VARCHAR(100)");
    let derived2 = ColumnRef::new("db3", "contacts", "given_name", "VARCHAR(100)");

    let event1 = ColumnLineageEvent::new(
        vec![source.clone()],
        derived1.clone(),
        "fname = first_name".to_string(),
        TransformationType::DirectCopy,
        "job-1".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    let event2 = ColumnLineageEvent::new(
        vec![source.clone()],
        derived2.clone(),
        "given_name = first_name".to_string(),
        TransformationType::DirectCopy,
        "job-2".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    store
        .record_column_lineage_batch(vec![event1, event2])
        .await
        .unwrap();

    // Get derived columns
    let derived = store.get_derived_columns(&source).await.unwrap();

    assert_eq!(derived.len(), 2);
    assert!(derived.contains(&derived1));
    assert!(derived.contains(&derived2));
}

#[tokio::test]
async fn test_transformation_type_filtering() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    // Create columns with different transformation types
    let source = ColumnRef::new("db1", "orders", "amount", "DECIMAL");
    let agg_target = ColumnRef::new("db2", "sales", "total_amount", "DECIMAL");

    let agg_event = ColumnLineageEvent::new(
        vec![source.clone()],
        agg_target.clone(),
        "total_amount = SUM(amount)".to_string(),
        TransformationType::Aggregation {
            function: "SUM".to_string(),
            group_by: Some(vec!["customer_id".to_string()]),
        },
        "job-1".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    store.record_column_lineage(agg_event).await.unwrap();
    store.flush().await.unwrap();

    // Find columns by transformation type
    let agg_transform = TransformationType::Aggregation {
        function: "SUM".to_string(),
        group_by: Some(vec!["customer_id".to_string()]),
    };

    let columns = store
        .find_columns_by_transformation(&agg_transform)
        .await
        .unwrap();

    assert!(columns.len() > 0);
    assert!(columns.contains(&agg_target));
}

#[tokio::test]
async fn test_critical_dependency_detection() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    // Create columns in different environments
    let staging_col = ColumnRef::new("staging-db", "temp_table", "value", "INT");
    let prod_col = ColumnRef::new("prod-db", "fact_sales", "revenue", "INT").with_schema("public");

    let event = ColumnLineageEvent::new(
        vec![staging_col.clone()],
        prod_col.clone(),
        "revenue = value".to_string(),
        TransformationType::DirectCopy,
        "job-1".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    store.record_column_lineage(event).await.unwrap();
    store.flush().await.unwrap();

    // Analyze impact on staging column
    let impact = store.analyze_column_impact(&staging_col).await.unwrap();

    // The prod column should be identified as critical
    assert_eq!(impact.critical_dependencies.len(), 1);
    assert_eq!(impact.critical_dependencies[0].datasource_id, "prod-db");
}

#[tokio::test]
async fn test_search_column_lineage() {
    let temp_dir = TempDir::new().unwrap();
    let store = ColumnLineageStore::new(temp_dir.path()).unwrap();

    let source = ColumnRef::new("db1", "customers", "customer_email", "VARCHAR");
    let target = ColumnRef::new("db2", "users", "email", "VARCHAR");

    let event = ColumnLineageEvent::new(
        vec![source],
        target.clone(),
        "email = customer_email".to_string(),
        TransformationType::DirectCopy,
        "job-1".to_string(),
        "tenant-1".to_string(),
        "system".to_string(),
    );

    store.record_column_lineage(event).await.unwrap();
    store.flush().await.unwrap();

    // Search by pattern
    let results = store
        .search_column_lineage("db2.users.email")
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].target_column, target);
}
