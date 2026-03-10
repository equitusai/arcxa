//! Integration tests for Schema Evolution tracking
//!
//! Tests the complete schema evolution tracking flow including:
//! - Recording schema change events
//! - Querying datasource and table change history
//! - Saving and retrieving schema versions
//! - Schema drift analysis between versions
//! - Migration impact analysis

use graphica_coordinator::storage::schema_evolution_store::SchemaEvolutionStore;
use graphica_core::core::lineage::schema_evolution::{
    ColumnSchema, SchemaChangeEvent, SchemaChangeType, SchemaElement, SchemaVersion, TableSchema,
};
use tempfile::TempDir;

#[tokio::test]
async fn test_record_and_query_schema_changes() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create a schema change event
    let event = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnAdded,
        "migration-001",
        "tenant-1",
    )
    .with_column("email")
    .with_after_state(SchemaElement::column("email", "VARCHAR(255)", false));

    // Record the change
    store.record_schema_change(event.clone()).unwrap();

    // Query changes for the datasource
    let changes = store
        .get_datasource_schema_changes("postgres-prod")
        .unwrap();

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].table_name, "users");
    assert_eq!(changes[0].datasource_id, "postgres-prod");
}

#[tokio::test]
async fn test_table_specific_schema_changes() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create changes for different tables
    let event1 = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnAdded,
        "system",
        "tenant-1",
    );

    let event2 = SchemaChangeEvent::new(
        "postgres-prod",
        "orders",
        SchemaChangeType::TableAdded,
        "system",
        "tenant-1",
    );

    store.record_schema_change(event1).unwrap();
    store.record_schema_change(event2).unwrap();

    // Query changes for specific table
    let user_changes = store
        .get_table_schema_changes("postgres-prod", "users")
        .unwrap();

    assert_eq!(user_changes.len(), 1);
    assert_eq!(user_changes[0].table_name, "users");

    let order_changes = store
        .get_table_schema_changes("postgres-prod", "orders")
        .unwrap();

    assert_eq!(order_changes.len(), 1);
    assert_eq!(order_changes[0].table_name, "orders");
}

#[tokio::test]
async fn test_breaking_changes_filter() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Breaking change: column dropped
    let breaking_event = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnDropped,
        "system",
        "tenant-1",
    );

    // Non-breaking change: column added
    let non_breaking_event = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnAdded,
        "system",
        "tenant-1",
    );

    store.record_schema_change(breaking_event).unwrap();
    store.record_schema_change(non_breaking_event).unwrap();

    // Query only breaking changes
    let breaking_changes = store.get_breaking_changes("postgres-prod").unwrap();

    assert_eq!(breaking_changes.len(), 1);
    assert!(breaking_changes[0].is_breaking);
    assert!(matches!(
        breaking_changes[0].change_type,
        SchemaChangeType::ColumnDropped
    ));
}

#[tokio::test]
async fn test_schema_version_snapshot() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create a schema version snapshot
    let version = SchemaVersion {
        version_id: "v1.0.0".to_string(),
        datasource_id: "postgres-prod".to_string(),
        schema_name: Some("public".to_string()),
        created_at: chrono::Utc::now(),
        migration_id: Some("migration-001".to_string()),
        tables: vec![TableSchema {
            name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".to_string(),
                    data_type: "INTEGER".to_string(),
                    nullable: false,
                    default_value: None,
                    position: 1,
                    comment: None,
                },
                ColumnSchema {
                    name: "email".to_string(),
                    data_type: "VARCHAR(255)".to_string(),
                    nullable: false,
                    default_value: None,
                    position: 2,
                    comment: None,
                },
            ],
            primary_key: Some(vec!["id".to_string()]),
            foreign_keys: vec![],
            indexes: vec![],
            comment: None,
        }],
        previous_version: None,
        git_commit: None,
        tags: vec!["production".to_string()],
    };

    // Save the version
    store.save_schema_version(version.clone()).unwrap();

    // Retrieve the version
    let retrieved = store.get_schema_version("v1.0.0").unwrap();

    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.version_id, "v1.0.0");
    assert_eq!(retrieved.tables.len(), 1);
    assert_eq!(retrieved.tables[0].name, "users");
    assert_eq!(retrieved.tables[0].columns.len(), 2);
}

#[tokio::test]
async fn test_get_latest_schema_version() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create multiple versions with different timestamps
    for i in 1..=3 {
        let version = SchemaVersion {
            version_id: format!("v1.0.{}", i),
            datasource_id: "postgres-prod".to_string(),
            schema_name: None,
            created_at: chrono::Utc::now() + chrono::Duration::seconds(i),
            migration_id: None,
            tables: vec![],
            previous_version: if i > 1 {
                Some(format!("v1.0.{}", i - 1))
            } else {
                None
            },
            git_commit: None,
            tags: vec![],
        };

        store.save_schema_version(version).unwrap();
    }

    // Get the latest version
    let latest = store.get_latest_schema_version("postgres-prod").unwrap();

    assert!(latest.is_some());
    assert_eq!(latest.unwrap().version_id, "v1.0.3");
}

#[tokio::test]
async fn test_schema_drift_analysis() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create source version
    let source_version = SchemaVersion {
        version_id: "v1.0.0".to_string(),
        datasource_id: "postgres-prod".to_string(),
        schema_name: None,
        created_at: chrono::Utc::now(),
        migration_id: None,
        tables: vec![],
        previous_version: None,
        git_commit: None,
        tags: vec![],
    };

    store.save_schema_version(source_version).unwrap();

    // Record some changes
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    let change = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnAdded,
        "system",
        "tenant-1",
    );

    store.record_schema_change(change).unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Create target version
    let target_version = SchemaVersion {
        version_id: "v1.1.0".to_string(),
        datasource_id: "postgres-prod".to_string(),
        schema_name: None,
        created_at: chrono::Utc::now(),
        migration_id: None,
        tables: vec![],
        previous_version: Some("v1.0.0".to_string()),
        git_commit: None,
        tags: vec![],
    };

    store.save_schema_version(target_version).unwrap();

    // Analyze drift
    let drift = store.analyze_schema_drift("v1.0.0", "v1.1.0").unwrap();

    assert_eq!(drift.source_version_id, "v1.0.0");
    assert_eq!(drift.target_version_id, "v1.1.0");
    assert!(drift.non_breaking_changes_count > 0);
}

#[tokio::test]
async fn test_migration_impact_analysis() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create a breaking change
    let change = SchemaChangeEvent::new(
        "postgres-prod",
        "users",
        SchemaChangeType::ColumnDropped,
        "system",
        "tenant-1",
    )
    .with_column("old_field");

    // Analyze impact
    let impact = store.analyze_migration_impact(&change).unwrap();

    assert!(impact.change.is_breaking);
    assert!(matches!(
        impact.risk_level,
        graphica_core::core::lineage::schema_evolution::RiskLevel::Critical
    ));
    assert!(!impact.migration_steps.is_empty());
}

#[tokio::test]
async fn test_search_schema_changes() {
    let temp_dir = TempDir::new().unwrap();
    let store = SchemaEvolutionStore::open(temp_dir.path()).unwrap();

    // Create events with different table names
    let event1 = SchemaChangeEvent::new(
        "postgres-prod",
        "user_profiles",
        SchemaChangeType::TableAdded,
        "system",
        "tenant-1",
    );

    let event2 = SchemaChangeEvent::new(
        "postgres-prod",
        "customer_data",
        SchemaChangeType::TableAdded,
        "system",
        "tenant-1",
    );

    store.record_schema_change(event1).unwrap();
    store.record_schema_change(event2).unwrap();

    // Search for tables matching "user"
    let results = store.search_schema_changes("user").unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].table_name, "user_profiles");
}
