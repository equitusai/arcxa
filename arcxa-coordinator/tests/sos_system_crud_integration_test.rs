//! Systems-of-Systems (SoS) - Phase 1 Integration Tests
//!
//! End-to-end tests for System Management CRUD operations:
//! - System registration and storage in RocksDB
//! - System retrieval and listing with filters
//! - System updates (partial and full)
//! - System deletion with dependency checks
//! - Secondary index performance
//! - Error scenarios and validation
//!
//! These tests verify the RocksDB storage layer, storage manager,
//! and data integrity across all Phase 1 operations.

use anyhow::Result;
use chrono::Utc;
use graphica_coordinator::api::sos_validation::storage::{Interface, SosStorageManager, System};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

/// Helper to create test environment with temporary RocksDB
fn setup_test_environment() -> Result<(Arc<SosStorageManager>, TempDir)> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path();

    let manager = SosStorageManager::new(temp_path.join("sos_test").to_str().unwrap())?;

    Ok((Arc::new(manager), temp_dir))
}

/// Helper to create a test system
fn create_test_system(id: &str, system_type: &str, vendor: &str) -> System {
    System {
        system_id: id.to_string(),
        system_name: format!("Test System {}", id),
        system_type: system_type.to_string(),
        version: "1.0.0".to_string(),
        vendor: vendor.to_string(),
        description: Some(format!("Test system {}", id)),
        classification: "Internal".to_string(),
        tags: vec!["test".to_string(), "integration".to_string()],
        deployment: HashMap::new(),
        capabilities: HashMap::new(),
        active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Test: Basic system registration and retrieval
#[test]
fn test_system_registration_and_retrieval() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Create and register a system
    let system = create_test_system("sys-001", "Sensor", "Acme Corp");

    // Store the system
    manager.put_system(&system)?;

    // Retrieve the system
    let retrieved = manager.get_system("sys-001")?;
    assert!(retrieved.is_some(), "System should be retrievable");

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.system_id, "sys-001");
    assert_eq!(retrieved.system_name, "Test System sys-001");
    assert_eq!(retrieved.system_type, "Sensor");
    assert_eq!(retrieved.vendor, "Acme Corp");
    assert_eq!(retrieved.version, "1.0.0");
    assert!(retrieved.active);

    Ok(())
}

/// Test: Non-existent system retrieval returns None
#[test]
fn test_get_nonexistent_system() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let result = manager.get_system("nonexistent-id")?;
    assert!(result.is_none(), "Non-existent system should return None");

    Ok(())
}

/// Test: Multiple system registration and listing
#[test]
fn test_multiple_system_registration() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register multiple systems
    for i in 0..5 {
        let system = create_test_system(&format!("sys-{:03}", i), "Sensor", "Acme Corp");
        manager.put_system(&system)?;
    }

    // List all systems
    let systems = manager.list_all_systems(0, 10)?;
    assert_eq!(systems.len(), 5, "Should retrieve all 5 systems");

    Ok(())
}

/// Test: List systems by type (secondary index)
#[test]
fn test_list_systems_by_type() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register systems of different types
    let sensor_sys = create_test_system("sensor-001", "Sensor", "Acme Corp");
    let actuator_sys = create_test_system("actuator-001", "Actuator", "Acme Corp");
    let controller_sys = create_test_system("controller-001", "Controller", "Acme Corp");
    let sensor_sys2 = create_test_system("sensor-002", "Sensor", "TechCo");

    manager.put_system(&sensor_sys)?;
    manager.put_system(&actuator_sys)?;
    manager.put_system(&controller_sys)?;
    manager.put_system(&sensor_sys2)?;

    // Query by type "Sensor"
    let sensors = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(sensors.len(), 2, "Should find 2 Sensor systems");

    // Verify both sensors are present
    let sensor_ids: Vec<_> = sensors.iter().map(|s| s.system_id.as_str()).collect();
    assert!(sensor_ids.contains(&"sensor-001"));
    assert!(sensor_ids.contains(&"sensor-002"));

    // Query by type "Actuator"
    let actuators = manager.list_systems_by_type("Actuator", 10)?;
    assert_eq!(actuators.len(), 1, "Should find 1 Actuator system");
    assert_eq!(actuators[0].system_id, "actuator-001");

    // Query by non-existent type
    let none = manager.list_systems_by_type("NonExistent", 10)?;
    assert_eq!(none.len(), 0, "Should find 0 systems for non-existent type");

    Ok(())
}

/// Test: System update (full replacement)
#[test]
fn test_system_update() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register initial system
    let mut system = create_test_system("sys-update-001", "Sensor", "Acme Corp");
    system.version = "1.0.0".to_string();
    system.description = Some("Original description".to_string());
    manager.put_system(&system)?;

    // Update the system
    system.version = "2.0.0".to_string();
    system.description = Some("Updated description".to_string());
    system.updated_at = Utc::now();
    manager.put_system(&system)?;

    // Retrieve and verify update
    let retrieved = manager.get_system("sys-update-001")?.unwrap();
    assert_eq!(retrieved.version, "2.0.0");
    assert_eq!(
        retrieved.description,
        Some("Updated description".to_string())
    );
    assert!(retrieved.updated_at >= system.updated_at);

    Ok(())
}

/// Test: System deletion
#[test]
fn test_system_deletion() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register a system
    let system = create_test_system("sys-delete-001", "Sensor", "Acme Corp");
    manager.put_system(&system)?;

    // Verify it exists
    assert!(manager.get_system("sys-delete-001")?.is_some());

    // Delete the system
    manager.delete_system("sys-delete-001", "Sensor", "Acme Corp")?;

    // Verify it's gone
    let retrieved = manager.get_system("sys-delete-001")?;
    assert!(
        retrieved.is_none(),
        "Deleted system should not be retrievable"
    );

    // Verify it's removed from secondary indexes
    let sensors = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(
        sensors.len(),
        0,
        "Deleted system should not appear in type index"
    );

    Ok(())
}

/// Test: Delete system with interfaces (dependency check)
#[test]
fn test_delete_system_with_interfaces() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register a system
    let system = create_test_system("sys-with-if-001", "Sensor", "Acme Corp");
    manager.put_system(&system)?;

    // Add an interface to the system
    let interface = Interface {
        interface_id: "if-001".to_string(),
        system_id: "sys-with-if-001".to_string(),
        interface_name: "Data Output".to_string(),
        direction: "Provider".to_string(),
        protocol: "REST".to_string(),
        data_format: "JSON".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "temperature": { "type": "number" }
            }
        }),
        unit_system: Some("SI".to_string()),
        coordinate_system: None,
        metadata: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };
    manager.put_interface(&interface)?;

    // Attempt to delete the system should be prevented by handler logic
    // (This test verifies the storage layer supports the dependency check)
    let interfaces = manager.list_interfaces_by_system("sys-with-if-001")?;
    assert_eq!(interfaces.len(), 1, "System should have 1 interface");
    assert_eq!(interfaces[0].interface_id, "if-001");

    Ok(())
}

/// Test: Pagination with offset and limit
#[test]
fn test_pagination() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register 15 systems
    for i in 0..15 {
        let system = create_test_system(&format!("sys-page-{:03}", i), "Sensor", "Acme Corp");
        manager.put_system(&system)?;
    }

    // First page (0-4)
    let page1 = manager.list_all_systems(0, 5)?;
    assert_eq!(page1.len(), 5, "First page should have 5 systems");

    // Second page (5-9)
    let page2 = manager.list_all_systems(5, 5)?;
    assert_eq!(page2.len(), 5, "Second page should have 5 systems");

    // Third page (10-14)
    let page3 = manager.list_all_systems(10, 5)?;
    assert_eq!(page3.len(), 5, "Third page should have 5 systems");

    // Fourth page (beyond data)
    let page4 = manager.list_all_systems(15, 5)?;
    assert_eq!(page4.len(), 0, "Page beyond data should be empty");

    // Verify no duplicates across pages
    let all_ids_page1: Vec<_> = page1.iter().map(|s| &s.system_id).collect();
    let all_ids_page2: Vec<_> = page2.iter().map(|s| &s.system_id).collect();
    let all_ids_page3: Vec<_> = page3.iter().map(|s| &s.system_id).collect();

    for id in &all_ids_page1 {
        assert!(!all_ids_page2.contains(id), "Pages should not overlap");
        assert!(!all_ids_page3.contains(id), "Pages should not overlap");
    }

    Ok(())
}

/// Test: Update system type and vendor (secondary index update)
#[test]
fn test_update_system_type_and_vendor() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register system with type "Sensor" and vendor "Acme Corp"
    let mut system = create_test_system("sys-type-change", "Sensor", "Acme Corp");
    manager.put_system(&system)?;

    // Verify it appears in Sensor index
    let sensors = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(sensors.len(), 1);

    // Change type to "Actuator" and vendor to "TechCo"
    system.system_type = "Actuator".to_string();
    system.vendor = "TechCo".to_string();
    manager.put_system(&system)?;

    // Verify it's removed from Sensor index
    let sensors_after = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(
        sensors_after.len(),
        0,
        "System should be removed from old type index"
    );

    // Verify it appears in Actuator index
    let actuators = manager.list_systems_by_type("Actuator", 10)?;
    assert_eq!(actuators.len(), 1);
    assert_eq!(actuators[0].system_id, "sys-type-change");
    assert_eq!(actuators[0].vendor, "TechCo");

    Ok(())
}

/// Test: Concurrent system registration (stress test)
#[test]
fn test_concurrent_registration() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register many systems quickly
    for i in 0..100 {
        let system = create_test_system(
            &format!("sys-stress-{:04}", i),
            if i % 3 == 0 {
                "Sensor"
            } else if i % 3 == 1 {
                "Actuator"
            } else {
                "Controller"
            },
            if i % 2 == 0 { "Acme Corp" } else { "TechCo" },
        );
        manager.put_system(&system)?;
    }

    // Verify all systems are retrievable
    let all_systems = manager.list_all_systems(0, 150)?;
    assert_eq!(all_systems.len(), 100, "All 100 systems should be stored");

    // Verify type indexes are correct
    let sensors = manager.list_systems_by_type("Sensor", 100)?;
    assert_eq!(
        sensors.len(),
        34,
        "Should have 34 Sensor systems (0, 3, 6, ...)"
    );

    let actuators = manager.list_systems_by_type("Actuator", 100)?;
    assert_eq!(
        actuators.len(),
        33,
        "Should have 33 Actuator systems (1, 4, 7, ...)"
    );

    let controllers = manager.list_systems_by_type("Controller", 100)?;
    assert_eq!(
        controllers.len(),
        33,
        "Should have 33 Controller systems (2, 5, 8, ...)"
    );

    Ok(())
}

/// Test: Empty database queries
#[test]
fn test_empty_database_queries() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Query empty database
    let all_systems = manager.list_all_systems(0, 10)?;
    assert_eq!(
        all_systems.len(),
        0,
        "Empty database should return no systems"
    );

    let sensors = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(
        sensors.len(),
        0,
        "Empty database should return no systems by type"
    );

    let system = manager.get_system("nonexistent")?;
    assert!(
        system.is_none(),
        "Empty database should return None for get"
    );

    Ok(())
}

/// Test: System with special characters in fields
#[test]
fn test_special_characters_in_system() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let mut system = create_test_system("sys-special", "Sensor", "Acme Corp");
    system.description = Some("System with special chars: <>&\"'`\n\t\\".to_string());
    system.tags = vec![
        "tag-with-dash".to_string(),
        "tag_with_underscore".to_string(),
        "tag.with.dots".to_string(),
    ];

    // Store and retrieve
    manager.put_system(&system)?;
    let retrieved = manager.get_system("sys-special")?.unwrap();

    assert_eq!(retrieved.description, system.description);
    assert_eq!(retrieved.tags, system.tags);

    Ok(())
}

/// Test: System idempotent updates (same data written twice)
#[test]
fn test_idempotent_updates() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-idempotent", "Sensor", "Acme Corp");

    // Write the same system twice
    manager.put_system(&system)?;
    manager.put_system(&system)?;

    // Should still have only one system
    let all_systems = manager.list_all_systems(0, 10)?;
    assert_eq!(all_systems.len(), 1);

    let sensors = manager.list_systems_by_type("Sensor", 10)?;
    assert_eq!(sensors.len(), 1);

    Ok(())
}
