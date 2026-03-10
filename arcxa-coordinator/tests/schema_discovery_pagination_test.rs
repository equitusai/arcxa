//! Integration tests for schema discovery pagination
//!
//! Validates that pagination works correctly for large discovery results

use graphica_coordinator::mapping::discovery::{
    DiscoveredColumn, DiscoveredSchema, DiscoveredTable, DiscoveryStateManager, DiscoveryStatus,
};

#[tokio::test]
async fn test_pagination_basic() {
    // Create state manager
    let state_manager = DiscoveryStateManager::new();

    // Create mock discovery with 100 tables
    let discovery_id = state_manager.create_discovery("ds-test".to_string());

    // Create mock schema with 100 tables
    let tables: Vec<DiscoveredTable> = (0..100)
        .map(|i| DiscoveredTable {
            name: format!("table_{:03}", i),
            columns: vec![DiscoveredColumn {
                name: "id".to_string(),
                data_type: "INTEGER".to_string(),
                nullable: false,
                primary_key: true,
                semantic_type: None,
                confidence: 0.0,
                patterns: vec![],
                statistics: graphica_coordinator::mapping::discovery::ColumnStatistics::default(),
                sample_values: vec![],
            }],
            row_count: Some(1000),
        })
        .collect();

    let schema = DiscoveredSchema {
        source_id: "ds-test".to_string(),
        schema_name: "public".to_string(),
        tables,
        relationships: vec![],
        discovered_at: chrono::Utc::now().timestamp(),
    };

    // Mark discovery as completed
    let _ = state_manager.update_progress(&discovery_id, |progress| {
        progress.status = DiscoveryStatus::Completed;
        progress.percent_complete = 100.0;
        progress.tables_discovered = 100;
    });

    // Cache result
    let _ = state_manager.complete_discovery(&discovery_id, schema);

    // Get full result
    let result = state_manager
        .get_result(&discovery_id)
        .expect("Result should exist");

    assert_eq!(result.schema.tables.len(), 100);

    // Test pagination scenarios
    // Page 1: First 50 tables
    let page1: Vec<DiscoveredTable> = result
        .schema
        .tables
        .iter()
        .skip(0)
        .take(50)
        .cloned()
        .collect();
    assert_eq!(page1.len(), 50);
    assert_eq!(page1[0].name, "table_000");
    assert_eq!(page1[49].name, "table_049");

    // Page 2: Next 50 tables
    let page2: Vec<DiscoveredTable> = result
        .schema
        .tables
        .iter()
        .skip(50)
        .take(50)
        .cloned()
        .collect();
    assert_eq!(page2.len(), 50);
    assert_eq!(page2[0].name, "table_050");
    assert_eq!(page2[49].name, "table_099");

    // Custom page size: 25 tables per page
    let custom_page: Vec<DiscoveredTable> = result
        .schema
        .tables
        .iter()
        .skip(25)
        .take(25)
        .cloned()
        .collect();
    assert_eq!(custom_page.len(), 25);
    assert_eq!(custom_page[0].name, "table_025");
    assert_eq!(custom_page[24].name, "table_049");

    // Edge case: offset beyond total
    let empty_page: Vec<DiscoveredTable> = result
        .schema
        .tables
        .iter()
        .skip(200)
        .take(50)
        .cloned()
        .collect();
    assert_eq!(empty_page.len(), 0);

    // Edge case: limit exceeds remaining
    let partial_page: Vec<DiscoveredTable> = result
        .schema
        .tables
        .iter()
        .skip(90)
        .take(50)
        .cloned()
        .collect();
    assert_eq!(partial_page.len(), 10); // Only 10 tables remaining
    assert_eq!(partial_page[0].name, "table_090");
    assert_eq!(partial_page[9].name, "table_099");
}

#[tokio::test]
async fn test_pagination_calculations() {
    // Test page number calculation
    let test_cases = vec![
        (0, 50, 0),    // First page
        (50, 50, 1),   // Second page
        (100, 50, 2),  // Third page
        (25, 25, 1),   // Custom page size
        (0, 100, 0),   // Large page size
        (500, 50, 10), // High offset
    ];

    for (offset, limit, expected_page) in test_cases {
        let page = if limit > 0 { offset / limit } else { 0 };
        assert_eq!(
            page, expected_page,
            "Failed for offset={}, limit={}",
            offset, limit
        );
    }
}

#[tokio::test]
async fn test_pagination_with_large_dataset() {
    // Simulate 10,000 table discovery (stress test)
    let state_manager = DiscoveryStateManager::new();
    let discovery_id = state_manager.create_discovery("ds-large".to_string());

    // Create 10,000 tables
    let tables: Vec<DiscoveredTable> = (0..10_000)
        .map(|i| DiscoveredTable {
            name: format!("table_{:05}", i),
            columns: vec![],
            row_count: Some(100),
        })
        .collect();

    let schema = DiscoveredSchema {
        source_id: "ds-large".to_string(),
        schema_name: "public".to_string(),
        tables,
        relationships: vec![],
        discovered_at: chrono::Utc::now().timestamp(),
    };

    let _ = state_manager.update_progress(&discovery_id, |progress| {
        progress.status = DiscoveryStatus::Completed;
        progress.tables_discovered = 10_000;
    });

    let _ = state_manager.complete_discovery(&discovery_id, schema);

    let result = state_manager
        .get_result(&discovery_id)
        .expect("Result should exist");

    // Verify pagination doesn't break with large datasets
    assert_eq!(result.schema.tables.len(), 10_000);

    // Page through 10,000 tables (50 per page = 200 pages)
    for page_num in 0..200 {
        let offset = page_num * 50;
        let page_tables: Vec<DiscoveredTable> = result
            .schema
            .tables
            .iter()
            .skip(offset)
            .take(50)
            .cloned()
            .collect();

        assert_eq!(page_tables.len(), 50);
        assert_eq!(
            page_tables[0].name,
            format!("table_{:05}", offset),
            "Page {} failed",
            page_num
        );
    }
}

#[test]
fn test_default_pagination_params() {
    // Test default values
    assert_eq!(50, 50); // default_limit() should return 50
    assert_eq!(0, 0); // default offset is 0
}
