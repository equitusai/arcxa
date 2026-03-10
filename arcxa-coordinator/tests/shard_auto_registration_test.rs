//! Integration tests for shard auto-registration
//!
//! These tests verify the complete registration flow:
//! - New shard registration with coordinator
//! - Shard reconnection after restart
//! - Heartbeat protocol
//! - Topology queries
//! - Machine ID conflict detection

use anyhow::Result;
use graphica_coordinator::governance::distributed::{
    auto_registration::{AutoRegistrationHandler, RegistrationConfig},
    coordinator_proto::{
        coordinator_service_server::{CoordinatorService, CoordinatorServiceServer},
        shard_registration_request, ExistingShardReconnection, HealthStatus, HeartbeatRequest,
        NewShardRegistration, ShardCapabilities, ShardRegistrationRequest, ShardStatistics,
        TopologyRequest,
    },
    coordinator_service_impl::{CoordinatorServiceConfig, CoordinatorServiceImpl},
    shard_metadata::ShardId,
    shard_registry::ShardRegistry,
};
use std::sync::Arc;
use tempfile::TempDir;
use tonic::Request;
use uuid::Uuid;

/// Test helper to create a test coordinator service
async fn create_test_coordinator_service(temp_dir: &TempDir) -> Result<CoordinatorServiceImpl> {
    let registry_path = temp_dir.path().join("shard-registry");
    let registry = Arc::new(ShardRegistry::new(
        registry_path.to_str().unwrap(),
        3,  // max shards
        60, // heartbeat timeout
    )?);

    let service_config = CoordinatorServiceConfig {
        max_shards: 3,
        heartbeat_interval_secs: 60,
        ..Default::default()
    };

    CoordinatorServiceImpl::new(registry, service_config)
}

#[tokio::test]
async fn test_new_shard_registration() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    let machine_id = Uuid::new_v4().to_string();

    // Create registration request
    let request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::NewShard(
            NewShardRegistration {
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(),
                capabilities: Some(ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 500_000,
                    cpu_cores: 8,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec!["subject".to_string(), "predicate".to_string()],
                    network_bandwidth_mbps: 1000,
                }),
                data_path: String::new(),
                version: "0.1.0".to_string(),
                preferred_shard_id: 0,
            },
        )),
        auth_token: String::new(),
    });

    // Execute
    let response = service.register_shard(request).await?;
    let result = response.into_inner();

    // Verify
    assert!(result.success, "Registration should succeed");
    assert_eq!(result.assigned_shard_id, 0, "First shard should get ID 0");
    assert!(result.hash_range.is_some(), "Should have hash range");

    let hash_range = result.hash_range.unwrap();
    assert_eq!(hash_range.start, 0, "First shard starts at 0");
    assert!(hash_range.end > 0, "Should have non-zero end range");

    Ok(())
}

#[tokio::test]
async fn test_multiple_shard_registration() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    // Register 3 shards
    let mut shard_ids = vec![];

    for i in 0..3 {
        let machine_id = Uuid::new_v4().to_string();

        let request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: format!("localhost:910{}", i),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 1000,
                    }),
                    data_path: String::new(),
                    version: "0.1.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        let response = service.register_shard(request).await?;
        let result = response.into_inner();

        assert!(result.success);
        shard_ids.push(result.assigned_shard_id);
    }

    // Verify sequential IDs
    assert_eq!(shard_ids, vec![0, 1, 2], "Should assign sequential IDs");

    // Verify topology
    let topology_request = Request::new(TopologyRequest {
        include_inactive: true,
        include_stats: false,
    });

    let topology_response = service.get_topology(topology_request).await?;
    let topology = topology_response.into_inner();

    assert_eq!(topology.total_shards, 3, "Should have 3 registered shards");
    assert_eq!(topology.shards.len(), 3, "Should return 3 shard infos");

    Ok(())
}

#[tokio::test]
async fn test_shard_reconnection() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    let machine_id = Uuid::new_v4().to_string();

    // Initial registration
    let initial_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::NewShard(
            NewShardRegistration {
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(),
                capabilities: Some(ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 500_000,
                    cpu_cores: 8,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                }),
                data_path: String::new(),
                version: "0.1.0".to_string(),
                preferred_shard_id: 0,
            },
        )),
        auth_token: String::new(),
    });

    let initial_response = service.register_shard(initial_request).await?;
    let initial_result = initial_response.into_inner();
    let assigned_shard_id = initial_result.assigned_shard_id;

    // Reconnect with same machine_id
    let reconnect_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::Reconnect(
            ExistingShardReconnection {
                shard_id: assigned_shard_id,
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(), // Same address
                last_checkpoint: 0,
                version: "0.1.0".to_string(),
            },
        )),
        auth_token: String::new(),
    });

    let reconnect_response = service.register_shard(reconnect_request).await?;
    let reconnect_result = reconnect_response.into_inner();

    assert!(reconnect_result.success, "Reconnection should succeed");
    assert_eq!(
        reconnect_result.assigned_shard_id, assigned_shard_id,
        "Should get same shard ID"
    );

    Ok(())
}

#[tokio::test]
async fn test_machine_id_conflict_detection() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    let machine_id = Uuid::new_v4().to_string();

    // Register first time
    let first_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::NewShard(
            NewShardRegistration {
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(),
                capabilities: Some(ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 500_000,
                    cpu_cores: 8,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                }),
                data_path: String::new(),
                version: "0.1.0".to_string(),
                preferred_shard_id: 0,
            },
        )),
        auth_token: String::new(),
    });

    let first_response = service.register_shard(first_request).await?;
    assert!(first_response.into_inner().success);

    // Try to register again with SAME machine_id (should fail with error)
    let duplicate_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::NewShard(
            NewShardRegistration {
                machine_id: machine_id.clone(),        // Same machine ID
                address: "localhost:9101".to_string(), // Different address
                capabilities: Some(ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 500_000,
                    cpu_cores: 8,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                }),
                data_path: String::new(),
                version: "0.1.0".to_string(),
                preferred_shard_id: 0,
            },
        )),
        auth_token: String::new(),
    });

    let duplicate_response = service.register_shard(duplicate_request).await;
    assert!(
        duplicate_response.is_err(),
        "Duplicate machine_id should return an error"
    );

    let error = duplicate_response.unwrap_err();
    assert_eq!(
        error.code(),
        tonic::Code::AlreadyExists,
        "Should return AlreadyExists status"
    );
    assert!(
        error.message().contains("already registered"),
        "Error message should mention duplicate: {}",
        error.message()
    );

    Ok(())
}

#[tokio::test]
async fn test_heartbeat_protocol() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    let machine_id = Uuid::new_v4().to_string();

    // Register shard first
    let register_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::NewShard(
            NewShardRegistration {
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(),
                capabilities: Some(ShardCapabilities {
                    max_memory_mb: 8192,
                    disk_space_mb: 500_000,
                    cpu_cores: 8,
                    supports_compression: true,
                    supports_encryption: false,
                    supported_indexes: vec![],
                    network_bandwidth_mbps: 1000,
                }),
                data_path: String::new(),
                version: "0.1.0".to_string(),
                preferred_shard_id: 0,
            },
        )),
        auth_token: String::new(),
    });

    let register_response = service.register_shard(register_request).await?;
    let shard_id = register_response.into_inner().assigned_shard_id;

    // Send heartbeat
    let heartbeat_request = Request::new(HeartbeatRequest {
        shard_id,
        machine_id: machine_id.clone(),
        stats: Some(ShardStatistics {
            triple_count: 1000,
            disk_usage_bytes: 5_000_000,
            memory_usage_bytes: 2_000_000,
            queries_processed: 50,
            inserts_processed: 100,
            deletes_processed: 10,
            p50_latency_ms: 5.0,
            p95_latency_ms: 15.0,
            p99_latency_ms: 25.0,
            query_errors: 0,
            insert_errors: 0,
            replication_lag_ms: 0,
        }),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        health: Some(HealthStatus {
            status: 0, // Healthy
            message: "All systems operational".to_string(),
            component_health: std::collections::HashMap::new(),
        }),
    });

    let heartbeat_response = service.heartbeat(heartbeat_request).await?;
    let heartbeat_result = heartbeat_response.into_inner();

    assert!(
        heartbeat_result.acknowledged,
        "Heartbeat should be acknowledged"
    );
    assert!(
        heartbeat_result.topology_version > 0,
        "Should have topology version"
    );

    Ok(())
}

#[tokio::test]
async fn test_invalid_shard_id_reconnection() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    let machine_id = Uuid::new_v4().to_string();

    // Try to reconnect with shard ID that was never registered
    let reconnect_request = Request::new(ShardRegistrationRequest {
        registration_type: Some(shard_registration_request::RegistrationType::Reconnect(
            ExistingShardReconnection {
                shard_id: 999, // Non-existent shard ID
                machine_id: machine_id.clone(),
                address: "localhost:9100".to_string(),
                last_checkpoint: 0,
                version: "0.1.0".to_string(),
            },
        )),
        auth_token: String::new(),
    });

    let reconnect_response = service.register_shard(reconnect_request).await;
    assert!(
        reconnect_response.is_err(),
        "Reconnection with invalid shard ID should return an error"
    );

    let error = reconnect_response.unwrap_err();
    assert_eq!(
        error.code(),
        tonic::Code::NotFound,
        "Should return NotFound status"
    );
    assert!(
        error.message().contains("not found"),
        "Error message should indicate shard not found: {}",
        error.message()
    );

    Ok(())
}

#[tokio::test]
async fn test_hash_range_distribution() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    // Register 3 shards
    for i in 0..3 {
        let machine_id = Uuid::new_v4().to_string();

        let request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: format!("localhost:910{}", i),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 1000,
                    }),
                    data_path: String::new(),
                    version: "0.1.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        service.register_shard(request).await?;
    }

    // Query the final topology to get the rebalanced hash ranges
    let topology_request = Request::new(TopologyRequest {
        include_inactive: true, // Include all shards regardless of status
        include_stats: true,
    });
    let topology_response = service.get_topology(topology_request).await?;
    let topology = topology_response.into_inner();

    // Extract and sort shard ranges by shard ID
    let mut shard_data: Vec<_> = topology
        .shards
        .iter()
        .map(|s| (s.shard_id, s.hash_range.as_ref().unwrap()))
        .collect();
    shard_data.sort_by_key(|(id, _)| *id);

    let hash_ranges: Vec<_> = shard_data.iter().map(|(_, range)| *range).collect();

    // Verify ranges are contiguous and cover full space
    assert_eq!(hash_ranges[0].start, 0, "First shard should start at 0");

    for i in 0..2 {
        assert_eq!(
            hash_ranges[i].end,
            hash_ranges[i + 1].start,
            "Ranges should be contiguous"
        );
    }

    assert_eq!(
        hash_ranges[2].end,
        u64::MAX,
        "Last shard should end at u64::MAX"
    );

    Ok(())
}

#[tokio::test]
async fn test_topology_query() -> Result<()> {
    // Setup
    let temp_dir = TempDir::new()?;
    let service = create_test_coordinator_service(&temp_dir).await?;

    // Register 2 shards
    for i in 0..2 {
        let machine_id = Uuid::new_v4().to_string();

        let request = Request::new(ShardRegistrationRequest {
            registration_type: Some(shard_registration_request::RegistrationType::NewShard(
                NewShardRegistration {
                    machine_id: machine_id.clone(),
                    address: format!("localhost:910{}", i),
                    capabilities: Some(ShardCapabilities {
                        max_memory_mb: 8192,
                        disk_space_mb: 500_000,
                        cpu_cores: 8,
                        supports_compression: true,
                        supports_encryption: false,
                        supported_indexes: vec![],
                        network_bandwidth_mbps: 1000,
                    }),
                    data_path: String::new(),
                    version: "0.1.0".to_string(),
                    preferred_shard_id: 0,
                },
            )),
            auth_token: String::new(),
        });

        service.register_shard(request).await?;
    }

    // Query topology
    let topology_request = Request::new(TopologyRequest {
        include_inactive: true, // Include all shards regardless of status
        include_stats: false,
    });

    let topology_response = service.get_topology(topology_request).await?;
    let topology = topology_response.into_inner();

    assert_eq!(topology.total_shards, 2);
    assert_eq!(topology.shards.len(), 2);

    // Verify each shard has required fields
    for shard_info in topology.shards {
        assert!(shard_info.shard_id < 2);
        assert!(!shard_info.machine_id.is_empty());
        assert!(shard_info.hash_range.is_some());
        assert!(!shard_info.leader_address.is_empty());
    }

    Ok(())
}
