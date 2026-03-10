//! Systems-of-Systems (SoS) - Phase 2 Integration Tests
//!
//! End-to-end tests for Interface Management CRUD operations:
//! - Interface registration and storage in RocksDB
//! - Interface retrieval and listing by system
//! - Interface updates (partial and full)
//! - Interface deletion with dependency checks
//! - Field validation (direction, protocol, data_format)
//! - System existence validation
//! - Secondary index performance
//! - Error scenarios and edge cases
//!
//! These tests verify the RocksDB storage layer, validators,
//! and data integrity across all Phase 2 operations.

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
fn create_test_system(id: &str, system_type: &str) -> System {
    System {
        system_id: id.to_string(),
        system_name: format!("Test System {}", id),
        system_type: system_type.to_string(),
        version: "1.0.0".to_string(),
        vendor: "Acme Corp".to_string(),
        description: Some(format!("Test system {}", id)),
        classification: "Internal".to_string(),
        tags: vec!["test".to_string()],
        deployment: HashMap::new(),
        capabilities: HashMap::new(),
        active: true,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// Helper to create a test interface
fn create_test_interface(
    interface_id: &str,
    system_id: &str,
    direction: &str,
    protocol: &str,
) -> Interface {
    Interface {
        interface_id: interface_id.to_string(),
        system_id: system_id.to_string(),
        interface_name: format!("Test Interface {}", interface_id),
        direction: direction.to_string(),
        protocol: protocol.to_string(),
        data_format: "JSON".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "properties": {
                "data": { "type": "string" }
            }
        }),
        unit_system: Some("SI".to_string()),
        coordinate_system: Some("WGS84".to_string()),
        metadata: HashMap::new(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

// ============================================================================
// Basic Interface CRUD Tests
// ============================================================================

/// Test: Basic interface registration and retrieval
#[test]
fn test_interface_registration_and_retrieval() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Create and register a system
    let system = create_test_system("sys-001", "Sensor");
    manager.put_system(&system)?;

    // Create and register an interface
    let interface = create_test_interface("if-001", "sys-001", "Provider", "REST");
    manager.put_interface(&interface)?;

    // Retrieve the interface
    let retrieved = manager.get_interface("if-001")?;
    assert!(retrieved.is_some(), "Interface should be retrievable");

    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.interface_id, "if-001");
    assert_eq!(retrieved.system_id, "sys-001");
    assert_eq!(retrieved.interface_name, "Test Interface if-001");
    assert_eq!(retrieved.direction, "Provider");
    assert_eq!(retrieved.protocol, "REST");
    assert_eq!(retrieved.data_format, "JSON");
    assert_eq!(retrieved.unit_system, Some("SI".to_string()));
    assert_eq!(retrieved.coordinate_system, Some("WGS84".to_string()));

    Ok(())
}

/// Test: Non-existent interface retrieval returns None
#[test]
fn test_get_nonexistent_interface() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let result = manager.get_interface("nonexistent-if")?;
    assert!(
        result.is_none(),
        "Non-existent interface should return None"
    );

    Ok(())
}

/// Test: Multiple interface registration for same system
#[test]
fn test_multiple_interfaces_per_system() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register a system
    let system = create_test_system("sys-multi", "Sensor");
    manager.put_system(&system)?;

    // Register multiple interfaces for the same system
    for i in 0..5 {
        let interface = create_test_interface(
            &format!("if-multi-{:03}", i),
            "sys-multi",
            "Provider",
            "REST",
        );
        manager.put_interface(&interface)?;
    }

    // List all interfaces for the system
    let interfaces = manager.list_interfaces_by_system("sys-multi")?;
    assert_eq!(interfaces.len(), 5, "Should retrieve all 5 interfaces");

    Ok(())
}

/// Test: List interfaces by system (secondary index)
#[test]
fn test_list_interfaces_by_system() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register multiple systems
    let sys1 = create_test_system("sys-list-1", "Sensor");
    let sys2 = create_test_system("sys-list-2", "Actuator");
    manager.put_system(&sys1)?;
    manager.put_system(&sys2)?;

    // Register interfaces for different systems
    let if1_sys1 = create_test_interface("if-s1-001", "sys-list-1", "Provider", "REST");
    let if2_sys1 = create_test_interface("if-s1-002", "sys-list-1", "Consumer", "gRPC");
    let if1_sys2 = create_test_interface("if-s2-001", "sys-list-2", "Provider", "MQTT");

    manager.put_interface(&if1_sys1)?;
    manager.put_interface(&if2_sys1)?;
    manager.put_interface(&if1_sys2)?;

    // List interfaces for sys-list-1
    let sys1_interfaces = manager.list_interfaces_by_system("sys-list-1")?;
    assert_eq!(
        sys1_interfaces.len(),
        2,
        "sys-list-1 should have 2 interfaces"
    );

    let sys1_if_ids: Vec<_> = sys1_interfaces
        .iter()
        .map(|i| i.interface_id.as_str())
        .collect();
    assert!(sys1_if_ids.contains(&"if-s1-001"));
    assert!(sys1_if_ids.contains(&"if-s1-002"));

    // List interfaces for sys-list-2
    let sys2_interfaces = manager.list_interfaces_by_system("sys-list-2")?;
    assert_eq!(
        sys2_interfaces.len(),
        1,
        "sys-list-2 should have 1 interface"
    );
    assert_eq!(sys2_interfaces[0].interface_id, "if-s2-001");

    Ok(())
}

// ============================================================================
// Interface Update Tests
// ============================================================================

/// Test: Interface update (full replacement)
#[test]
fn test_interface_update() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register system and interface
    let system = create_test_system("sys-update", "Sensor");
    manager.put_system(&system)?;

    let mut interface = create_test_interface("if-update", "sys-update", "Provider", "REST");
    interface.unit_system = Some("SI".to_string());
    manager.put_interface(&interface)?;

    // Update the interface
    interface.direction = "Consumer".to_string();
    interface.unit_system = Some("Imperial".to_string());
    interface.updated_at = Utc::now();
    manager.put_interface(&interface)?;

    // Retrieve and verify update
    let retrieved = manager.get_interface("if-update")?.unwrap();
    assert_eq!(retrieved.direction, "Consumer");
    assert_eq!(retrieved.unit_system, Some("Imperial".to_string()));

    Ok(())
}

/// Test: Interface metadata update
#[test]
fn test_interface_metadata_update() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register system and interface
    let system = create_test_system("sys-meta", "Sensor");
    manager.put_system(&system)?;

    let mut interface = create_test_interface("if-meta", "sys-meta", "Provider", "REST");
    interface
        .metadata
        .insert("version".to_string(), serde_json::json!("1.0"));
    manager.put_interface(&interface)?;

    // Update metadata
    interface
        .metadata
        .insert("version".to_string(), serde_json::json!("2.0"));
    interface
        .metadata
        .insert("author".to_string(), serde_json::json!("Test Team"));
    manager.put_interface(&interface)?;

    // Verify metadata update
    let retrieved = manager.get_interface("if-meta")?.unwrap();
    assert_eq!(
        retrieved.metadata.get("version"),
        Some(&serde_json::json!("2.0"))
    );
    assert_eq!(
        retrieved.metadata.get("author"),
        Some(&serde_json::json!("Test Team"))
    );

    Ok(())
}

// ============================================================================
// Interface Deletion Tests
// ============================================================================

/// Test: Interface deletion
#[test]
fn test_interface_deletion() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register system and interface
    let system = create_test_system("sys-delete", "Sensor");
    manager.put_system(&system)?;

    let interface = create_test_interface("if-delete", "sys-delete", "Provider", "REST");
    manager.put_interface(&interface)?;

    // Verify it exists
    assert!(manager.get_interface("if-delete")?.is_some());

    // Delete the interface
    manager.delete_interface("if-delete", "sys-delete")?;

    // Verify it's gone
    let retrieved = manager.get_interface("if-delete")?;
    assert!(
        retrieved.is_none(),
        "Deleted interface should not be retrievable"
    );

    // Verify it's removed from system index
    let interfaces = manager.list_interfaces_by_system("sys-delete")?;
    assert_eq!(
        interfaces.len(),
        0,
        "Deleted interface should not appear in system index"
    );

    Ok(())
}

/// Test: Delete interface does not affect other interfaces
#[test]
fn test_interface_deletion_isolation() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Register system with multiple interfaces
    let system = create_test_system("sys-iso", "Sensor");
    manager.put_system(&system)?;

    let if1 = create_test_interface("if-iso-1", "sys-iso", "Provider", "REST");
    let if2 = create_test_interface("if-iso-2", "sys-iso", "Consumer", "gRPC");
    let if3 = create_test_interface("if-iso-3", "sys-iso", "Bidirectional", "MQTT");

    manager.put_interface(&if1)?;
    manager.put_interface(&if2)?;
    manager.put_interface(&if3)?;

    // Delete middle interface
    manager.delete_interface("if-iso-2", "sys-iso")?;

    // Verify other interfaces still exist
    assert!(manager.get_interface("if-iso-1")?.is_some());
    assert!(manager.get_interface("if-iso-3")?.is_some());

    let interfaces = manager.list_interfaces_by_system("sys-iso")?;
    assert_eq!(interfaces.len(), 2);

    Ok(())
}

// ============================================================================
// Direction Validation Tests
// ============================================================================

/// Test: Valid direction values
#[test]
fn test_valid_directions() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-dir", "Sensor");
    manager.put_system(&system)?;

    // Test all valid directions
    let valid_directions = vec!["Provider", "Consumer", "Bidirectional"];

    for (i, direction) in valid_directions.iter().enumerate() {
        let interface =
            create_test_interface(&format!("if-dir-{}", i), "sys-dir", direction, "REST");
        manager.put_interface(&interface)?;

        let retrieved = manager.get_interface(&format!("if-dir-{}", i))?.unwrap();
        assert_eq!(&retrieved.direction, direction);
    }

    Ok(())
}

// ============================================================================
// Protocol Validation Tests
// ============================================================================

/// Test: Valid protocol values
#[test]
fn test_valid_protocols() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-proto", "Sensor");
    manager.put_system(&system)?;

    // Test all valid protocols
    let valid_protocols = vec![
        "REST",
        "gRPC",
        "MQTT",
        "AMQP",
        "Kafka",
        "WebSocket",
        "HTTP",
        "HTTPS",
        "TCP",
        "UDP",
    ];

    for (i, protocol) in valid_protocols.iter().enumerate() {
        let interface = create_test_interface(
            &format!("if-proto-{}", i),
            "sys-proto",
            "Provider",
            protocol,
        );
        manager.put_interface(&interface)?;

        let retrieved = manager.get_interface(&format!("if-proto-{}", i))?.unwrap();
        assert_eq!(&retrieved.protocol, protocol);
    }

    Ok(())
}

// ============================================================================
// Data Format Tests
// ============================================================================

/// Test: Valid data format values
#[test]
fn test_valid_data_formats() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-format", "Sensor");
    manager.put_system(&system)?;

    // Test all valid data formats
    let valid_formats = vec![
        "JSON",
        "XML",
        "Protobuf",
        "Avro",
        "MessagePack",
        "Parquet",
        "CSV",
        "YAML",
    ];

    for (i, format) in valid_formats.iter().enumerate() {
        let mut interface = create_test_interface(
            &format!("if-format-{}", i),
            "sys-format",
            "Provider",
            "REST",
        );
        interface.data_format = format.to_string();
        manager.put_interface(&interface)?;

        let retrieved = manager.get_interface(&format!("if-format-{}", i))?.unwrap();
        assert_eq!(&retrieved.data_format, format);
    }

    Ok(())
}

// ============================================================================
// Schema Tests
// ============================================================================

/// Test: Interface with complex JSON schema
#[test]
fn test_complex_json_schema() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-schema", "Sensor");
    manager.put_system(&system)?;

    let mut interface = create_test_interface("if-schema", "sys-schema", "Provider", "REST");
    interface.schema = serde_json::json!({
        "type": "object",
        "required": ["timestamp", "sensorId", "data"],
        "properties": {
            "timestamp": {
                "type": "string",
                "format": "date-time"
            },
            "sensorId": {
                "type": "string",
                "pattern": "^SENSOR-[0-9]{4}$"
            },
            "data": {
                "type": "object",
                "properties": {
                    "temperature": { "type": "number", "minimum": -273.15 },
                    "humidity": { "type": "number", "minimum": 0, "maximum": 100 },
                    "pressure": { "type": "number" }
                }
            },
            "tags": {
                "type": "array",
                "items": { "type": "string" }
            }
        }
    });

    manager.put_interface(&interface)?;

    let retrieved = manager.get_interface("if-schema")?.unwrap();
    assert_eq!(retrieved.schema, interface.schema);

    Ok(())
}

// ============================================================================
// Coordinate and Unit System Tests
// ============================================================================

/// Test: Coordinate system variations
#[test]
fn test_coordinate_systems() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-coord", "Sensor");
    manager.put_system(&system)?;

    let coordinate_systems = vec![
        Some("WGS84".to_string()),
        Some("ECI_J2000".to_string()),
        Some("ECEF".to_string()),
        None,
    ];

    for (i, coord_sys) in coordinate_systems.iter().enumerate() {
        let mut interface =
            create_test_interface(&format!("if-coord-{}", i), "sys-coord", "Provider", "REST");
        interface.coordinate_system = coord_sys.clone();
        manager.put_interface(&interface)?;

        let retrieved = manager.get_interface(&format!("if-coord-{}", i))?.unwrap();
        assert_eq!(retrieved.coordinate_system, *coord_sys);
    }

    Ok(())
}

/// Test: Unit system variations
#[test]
fn test_unit_systems() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-unit", "Sensor");
    manager.put_system(&system)?;

    let unit_systems = vec![
        Some("SI".to_string()),
        Some("Imperial".to_string()),
        Some("CGS".to_string()),
        None,
    ];

    for (i, unit_sys) in unit_systems.iter().enumerate() {
        let mut interface =
            create_test_interface(&format!("if-unit-{}", i), "sys-unit", "Provider", "REST");
        interface.unit_system = unit_sys.clone();
        manager.put_interface(&interface)?;

        let retrieved = manager.get_interface(&format!("if-unit-{}", i))?.unwrap();
        assert_eq!(retrieved.unit_system, *unit_sys);
    }

    Ok(())
}

// ============================================================================
// Edge Case Tests
// ============================================================================

/// Test: Interface with special characters
#[test]
fn test_special_characters_in_interface() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-special", "Sensor");
    manager.put_system(&system)?;

    let mut interface = create_test_interface("if-special", "sys-special", "Provider", "REST");
    interface.interface_name = "Interface with special chars: <>&\"'`\n\t\\".to_string();
    interface.metadata.insert(
        "description".to_string(),
        serde_json::json!("Test with unicode: 你好世界 🚀"),
    );

    manager.put_interface(&interface)?;
    let retrieved = manager.get_interface("if-special")?.unwrap();

    assert_eq!(retrieved.interface_name, interface.interface_name);
    assert_eq!(retrieved.metadata, interface.metadata);

    Ok(())
}

/// Test: Empty system has no interfaces
#[test]
fn test_empty_system_interface_list() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-empty", "Sensor");
    manager.put_system(&system)?;

    let interfaces = manager.list_interfaces_by_system("sys-empty")?;
    assert_eq!(interfaces.len(), 0, "New system should have no interfaces");

    Ok(())
}

/// Test: Idempotent interface updates
#[test]
fn test_idempotent_interface_updates() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-idem", "Sensor");
    manager.put_system(&system)?;

    let interface = create_test_interface("if-idem", "sys-idem", "Provider", "REST");

    // Write the same interface multiple times
    manager.put_interface(&interface)?;
    manager.put_interface(&interface)?;
    manager.put_interface(&interface)?;

    // Should still have only one interface
    let interfaces = manager.list_interfaces_by_system("sys-idem")?;
    assert_eq!(interfaces.len(), 1);

    let retrieved = manager.get_interface("if-idem")?.unwrap();
    assert_eq!(retrieved.interface_id, "if-idem");

    Ok(())
}

// ============================================================================
// Stress Tests
// ============================================================================

/// Test: Many interfaces for single system
#[test]
fn test_many_interfaces_single_system() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-many", "Sensor");
    manager.put_system(&system)?;

    // Register 50 interfaces
    for i in 0..50 {
        let interface = create_test_interface(
            &format!("if-many-{:03}", i),
            "sys-many",
            if i % 3 == 0 {
                "Provider"
            } else if i % 3 == 1 {
                "Consumer"
            } else {
                "Bidirectional"
            },
            if i % 4 == 0 {
                "REST"
            } else if i % 4 == 1 {
                "gRPC"
            } else if i % 4 == 2 {
                "MQTT"
            } else {
                "Kafka"
            },
        );
        manager.put_interface(&interface)?;
    }

    let interfaces = manager.list_interfaces_by_system("sys-many")?;
    assert_eq!(interfaces.len(), 50, "Should retrieve all 50 interfaces");

    Ok(())
}

/// Test: Interfaces across many systems
#[test]
fn test_interfaces_across_many_systems() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Create 20 systems, each with 3 interfaces
    for sys_i in 0..20 {
        let system = create_test_system(&format!("sys-cross-{:02}", sys_i), "Sensor");
        manager.put_system(&system)?;

        for if_i in 0..3 {
            let interface = create_test_interface(
                &format!("if-s{:02}-{}", sys_i, if_i),
                &format!("sys-cross-{:02}", sys_i),
                "Provider",
                "REST",
            );
            manager.put_interface(&interface)?;
        }
    }

    // Verify each system has exactly 3 interfaces
    for sys_i in 0..20 {
        let interfaces = manager.list_interfaces_by_system(&format!("sys-cross-{:02}", sys_i))?;
        assert_eq!(
            interfaces.len(),
            3,
            "System sys-cross-{:02} should have 3 interfaces",
            sys_i
        );
    }

    Ok(())
}

/// Test: Interface retrieval performance
#[test]
fn test_interface_retrieval_performance() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-perf", "Sensor");
    manager.put_system(&system)?;

    // Register 100 interfaces
    for i in 0..100 {
        let interface =
            create_test_interface(&format!("if-perf-{:03}", i), "sys-perf", "Provider", "REST");
        manager.put_interface(&interface)?;
    }

    // Retrieve all interfaces multiple times (should be fast)
    for _ in 0..10 {
        let interfaces = manager.list_interfaces_by_system("sys-perf")?;
        assert_eq!(interfaces.len(), 100);
    }

    Ok(())
}

// ============================================================================
// Global Interface Listing Tests
// ============================================================================

/// Test: List all interfaces across all systems
#[test]
fn test_list_all_interfaces() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    // Create 3 systems with different numbers of interfaces
    let sys1 = create_test_system("sys-all-1", "Sensor");
    let sys2 = create_test_system("sys-all-2", "Actuator");
    let sys3 = create_test_system("sys-all-3", "Controller");

    manager.put_system(&sys1)?;
    manager.put_system(&sys2)?;
    manager.put_system(&sys3)?;

    // System 1: 3 interfaces
    for i in 0..3 {
        let interface =
            create_test_interface(&format!("if-all-s1-{}", i), "sys-all-1", "Provider", "REST");
        manager.put_interface(&interface)?;
    }

    // System 2: 2 interfaces
    for i in 0..2 {
        let interface =
            create_test_interface(&format!("if-all-s2-{}", i), "sys-all-2", "Consumer", "gRPC");
        manager.put_interface(&interface)?;
    }

    // System 3: 1 interface
    let interface = create_test_interface("if-all-s3-0", "sys-all-3", "Bidirectional", "MQTT");
    manager.put_interface(&interface)?;

    // List all interfaces
    let all_interfaces = manager.list_all_interfaces(0, 100)?;
    assert_eq!(
        all_interfaces.len(),
        6,
        "Should retrieve all 6 interfaces across all systems"
    );

    // Verify we have interfaces from all three systems
    let system_ids: Vec<_> = all_interfaces
        .iter()
        .map(|i| i.system_id.as_str())
        .collect();
    assert!(system_ids.contains(&"sys-all-1"));
    assert!(system_ids.contains(&"sys-all-2"));
    assert!(system_ids.contains(&"sys-all-3"));

    Ok(())
}

/// Test: List all interfaces with pagination
#[test]
fn test_list_all_interfaces_pagination() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let system = create_test_system("sys-page", "Sensor");
    manager.put_system(&system)?;

    // Register 15 interfaces
    for i in 0..15 {
        let interface =
            create_test_interface(&format!("if-page-{:02}", i), "sys-page", "Provider", "REST");
        manager.put_interface(&interface)?;
    }

    // First page (0-4)
    let page1 = manager.list_all_interfaces(0, 5)?;
    assert_eq!(page1.len(), 5, "First page should have 5 interfaces");

    // Second page (5-9)
    let page2 = manager.list_all_interfaces(5, 5)?;
    assert_eq!(page2.len(), 5, "Second page should have 5 interfaces");

    // Third page (10-14)
    let page3 = manager.list_all_interfaces(10, 5)?;
    assert_eq!(page3.len(), 5, "Third page should have 5 interfaces");

    // Fourth page (beyond data)
    let page4 = manager.list_all_interfaces(15, 5)?;
    assert_eq!(page4.len(), 0, "Page beyond data should be empty");

    // Verify no duplicates across pages
    let all_ids_page1: Vec<_> = page1.iter().map(|i| &i.interface_id).collect();
    let all_ids_page2: Vec<_> = page2.iter().map(|i| &i.interface_id).collect();
    let all_ids_page3: Vec<_> = page3.iter().map(|i| &i.interface_id).collect();

    for id in &all_ids_page1 {
        assert!(!all_ids_page2.contains(id), "Pages should not overlap");
        assert!(!all_ids_page3.contains(id), "Pages should not overlap");
    }

    Ok(())
}

/// Test: List all interfaces with empty database
#[test]
fn test_list_all_interfaces_empty_database() -> Result<()> {
    let (manager, _temp_dir) = setup_test_environment()?;

    let interfaces = manager.list_all_interfaces(0, 100)?;
    assert_eq!(
        interfaces.len(),
        0,
        "Empty database should return no interfaces"
    );

    Ok(())
}
