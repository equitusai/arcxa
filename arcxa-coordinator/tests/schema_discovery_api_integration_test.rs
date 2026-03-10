//! Integration test for Schema Discovery REST API
//!
//! Tests the complete discovery flow:
//! 1. Start discovery
//! 2. Poll progress
//! 3. Retrieve result
//! 4. Test cancellation

use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

use graphica_coordinator::mapping::discovery::{
    DiscoveryOrchestrator, DiscoveryStateManager, DiscoveryStatus,
};
use graphica_core::catalog::connector::Credentials;
use graphica_core::catalog::types::DataSource;

#[tokio::test]
async fn test_discovery_state_lifecycle() -> Result<()> {
    // Initialize state manager
    let state_manager = DiscoveryStateManager::new();

    // Create discovery
    let discovery_id = state_manager.create_discovery("ds-test-123".to_string());
    assert!(!discovery_id.is_empty());

    // Check initial state
    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Queued);
    assert_eq!(progress.percent_complete, 0.0);
    assert_eq!(progress.datasource_id, "ds-test-123");

    // Start discovery
    state_manager.start_discovery(&discovery_id)?;
    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Running);

    // Update progress
    state_manager.update_progress(&discovery_id, |p| {
        p.current_step = "Introspecting tables".to_string();
        p.tables_discovered = 5;
        p.total_tables = Some(10);
        p.update_percent();
    })?;

    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.tables_discovered, 5);
    assert_eq!(progress.total_tables, Some(10));
    assert_eq!(progress.percent_complete, 50.0);

    // Complete discovery
    let schema = graphica_coordinator::mapping::discovery::DiscoveredSchema {
        source_id: "ds-test-123".to_string(),
        schema_name: "public".to_string(),
        tables: vec![],
        relationships: vec![],
        discovered_at: 0,
    };

    state_manager.complete_discovery(&discovery_id, schema)?;

    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Completed);
    assert_eq!(progress.percent_complete, 100.0);

    // Retrieve result
    let result = state_manager.get_result(&discovery_id).unwrap();
    assert_eq!(result.schema.source_id, "ds-test-123");

    Ok(())
}

#[tokio::test]
async fn test_discovery_cancellation() -> Result<()> {
    let state_manager = DiscoveryStateManager::new();

    let discovery_id = state_manager.create_discovery("ds-cancel-test".to_string());
    state_manager.start_discovery(&discovery_id)?;

    // Cancel discovery
    state_manager.cancel_discovery(&discovery_id)?;

    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Cancelled);

    Ok(())
}

#[tokio::test]
async fn test_discovery_failure() -> Result<()> {
    let state_manager = DiscoveryStateManager::new();

    let discovery_id = state_manager.create_discovery("ds-fail-test".to_string());
    state_manager.start_discovery(&discovery_id)?;

    // Fail discovery
    state_manager.fail_discovery(&discovery_id, "Connection timeout".to_string())?;

    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Failed);
    assert_eq!(progress.errors.len(), 1);
    assert_eq!(progress.errors[0], "Connection timeout");

    Ok(())
}

#[tokio::test]
async fn test_discovery_result_expiration() -> Result<()> {
    // Create state manager with 1 second TTL
    let state_manager = DiscoveryStateManager::with_ttl(1);

    let discovery_id = state_manager.create_discovery("ds-expire-test".to_string());

    let schema = graphica_coordinator::mapping::discovery::DiscoveredSchema {
        source_id: "ds-expire-test".to_string(),
        schema_name: "public".to_string(),
        tables: vec![],
        relationships: vec![],
        discovered_at: 0,
    };

    state_manager.complete_discovery(&discovery_id, schema)?;

    // Result should be available immediately
    assert!(state_manager.get_result(&discovery_id).is_some());

    // Wait for expiration
    sleep(Duration::from_secs(2)).await;

    // Result should be expired
    assert!(state_manager.get_result(&discovery_id).is_none());

    Ok(())
}

#[tokio::test]
async fn test_discovery_stats() -> Result<()> {
    let state_manager = DiscoveryStateManager::new();

    let id1 = state_manager.create_discovery("ds-1".to_string());
    let id2 = state_manager.create_discovery("ds-2".to_string());
    let id3 = state_manager.create_discovery("ds-3".to_string());
    let id4 = state_manager.create_discovery("ds-4".to_string());

    state_manager.start_discovery(&id1)?;
    state_manager.fail_discovery(&id2, "Error".to_string())?;
    state_manager.cancel_discovery(&id3)?;
    // id4 stays queued

    let stats = state_manager.get_stats();
    assert_eq!(stats.queued, 1);
    assert_eq!(stats.running, 1);
    assert_eq!(stats.failed, 1);
    assert_eq!(stats.cancelled, 1);

    Ok(())
}

#[tokio::test]
async fn test_list_discoveries_for_datasource() -> Result<()> {
    let state_manager = DiscoveryStateManager::new();

    state_manager.create_discovery("ds-alpha".to_string());
    state_manager.create_discovery("ds-alpha".to_string());
    state_manager.create_discovery("ds-beta".to_string());

    let alpha_discoveries = state_manager.list_discoveries_for_datasource("ds-alpha");
    assert_eq!(alpha_discoveries.len(), 2);

    let beta_discoveries = state_manager.list_discoveries_for_datasource("ds-beta");
    assert_eq!(beta_discoveries.len(), 1);

    Ok(())
}

#[tokio::test]
async fn test_cleanup_expired_results() -> Result<()> {
    let state_manager = DiscoveryStateManager::with_ttl(1);

    let id1 = state_manager.create_discovery("ds-1".to_string());
    let id2 = state_manager.create_discovery("ds-2".to_string());

    let schema = graphica_coordinator::mapping::discovery::DiscoveredSchema {
        source_id: "test".to_string(),
        schema_name: "public".to_string(),
        tables: vec![],
        relationships: vec![],
        discovered_at: 0,
    };

    state_manager.complete_discovery(&id1, schema.clone())?;
    state_manager.complete_discovery(&id2, schema)?;

    // Both results should be cached
    let stats = state_manager.get_stats();
    assert_eq!(stats.cached_results, 2);

    // Wait for expiration
    sleep(Duration::from_secs(2)).await;

    // Clean up expired results
    let removed = state_manager.cleanup_expired_results();
    assert_eq!(removed, 2);

    let stats = state_manager.get_stats();
    assert_eq!(stats.cached_results, 0);

    Ok(())
}

#[tokio::test]
async fn test_concurrent_progress_updates() -> Result<()> {
    let state_manager = Arc::new(DiscoveryStateManager::new());
    let discovery_id = state_manager.create_discovery("ds-concurrent".to_string());

    state_manager.start_discovery(&discovery_id)?;

    // Spawn multiple tasks updating progress concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let state_manager = state_manager.clone();
        let discovery_id = discovery_id.clone();

        handles.push(tokio::spawn(async move {
            state_manager.update_progress(&discovery_id, |p| {
                p.tables_discovered = i;
                p.current_step = format!("Step {}", i);
            })
        }));
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    // Verify state is consistent (no corruption)
    let progress = state_manager.get_progress(&discovery_id).unwrap();
    assert_eq!(progress.status, DiscoveryStatus::Running);
    assert!(progress.tables_discovered < 10);

    Ok(())
}

#[test]
fn test_discovery_progress_serialization() -> Result<()> {
    use chrono::Utc;
    use graphica_coordinator::mapping::discovery::DiscoveryProgress;

    let progress = DiscoveryProgress {
        discovery_id: "test-123".to_string(),
        datasource_id: "ds-456".to_string(),
        status: DiscoveryStatus::Running,
        current_step: "Introspecting".to_string(),
        tables_discovered: 10,
        total_tables: Some(20),
        percent_complete: 50.0,
        errors: vec![],
        started_at: Utc::now(),
        updated_at: Utc::now(),
        completed_at: None,
    };

    // Test JSON serialization
    let json = serde_json::to_string(&progress)?;
    let deserialized: DiscoveryProgress = serde_json::from_str(&json)?;

    assert_eq!(deserialized.discovery_id, "test-123");
    assert_eq!(deserialized.datasource_id, "ds-456");
    assert_eq!(deserialized.tables_discovered, 10);
    assert_eq!(deserialized.percent_complete, 50.0);

    Ok(())
}
