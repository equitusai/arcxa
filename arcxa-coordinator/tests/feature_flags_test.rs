//! Comprehensive tests for feature flags system
//!
//! Tests cover:
//! - Percentage-based rollout with hash distribution
//! - Tenant and dataset targeting (allowlist/denylist)
//! - Emergency kill switch functionality
//! - Concurrent flag evaluation
//! - Edge cases and resilience

use graphica_coordinator::storage::kafka::{FeatureFlagManager, FeatureFlags};
use std::sync::Arc;

#[test]
fn test_feature_flags_default() {
    let flags = FeatureFlags::default();

    // All features should be enabled by default
    assert!(flags.durable_writes.enabled);
    assert!(flags.circuit_breaker.enabled);
    assert!(flags.startup_recovery.enabled);
    assert!(flags.acknowledgment_tracking.enabled);
    assert!(flags.metrics.enabled);
    assert!(flags.tracing.enabled);

    // Default rollout percentage should be 100%
    assert_eq!(flags.durable_writes.rollout_percentage, 100);
}

#[test]
fn test_rollout_percentage_distribution() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 50;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    let mut enabled_count = 0;
    let total_events = 10000;

    // Test 10,000 events for statistical significance
    for i in 0..total_events {
        let event_id = format!("event_{}", i);
        if manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id) {
            enabled_count += 1;
        }
    }

    // Should be approximately 50% (±2% tolerance for 10k samples)
    let actual_percentage = (enabled_count as f64 / total_events as f64) * 100.0;
    assert!(
        actual_percentage >= 48.0 && actual_percentage <= 52.0,
        "Expected ~50% rollout, got {:.2}%",
        actual_percentage
    );
}

#[test]
fn test_rollout_percentage_25_percent() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 25;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    let mut enabled_count = 0;
    let total_events = 10000;

    for i in 0..total_events {
        let event_id = format!("event_{}", i);
        if manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id) {
            enabled_count += 1;
        }
    }

    // Should be approximately 25% (±2% tolerance)
    let actual_percentage = (enabled_count as f64 / total_events as f64) * 100.0;
    assert!(
        actual_percentage >= 23.0 && actual_percentage <= 27.0,
        "Expected ~25% rollout, got {:.2}%",
        actual_percentage
    );
}

#[test]
fn test_rollout_percentage_75_percent() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 75;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    let mut enabled_count = 0;
    let total_events = 10000;

    for i in 0..total_events {
        let event_id = format!("event_{}", i);
        if manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id) {
            enabled_count += 1;
        }
    }

    // Should be approximately 75% (±2% tolerance)
    let actual_percentage = (enabled_count as f64 / total_events as f64) * 100.0;
    assert!(
        actual_percentage >= 73.0 && actual_percentage <= 77.0,
        "Expected ~75% rollout, got {:.2}%",
        actual_percentage
    );
}

#[test]
fn test_tenant_allowlist() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 100;

    // Only enable for tenant_a
    flags
        .durable_writes
        .enabled_tenants
        .insert("tenant_a".to_string());

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // tenant_a should be enabled
    assert!(manager.is_durable_writes_enabled("tenant_a"));

    // tenant_b should be disabled (not in allowlist)
    assert!(!manager.is_durable_writes_enabled("tenant_b"));
}

#[test]
fn test_tenant_denylist() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 100;

    // Disable for tenant_a
    flags
        .durable_writes
        .disabled_tenants
        .insert("tenant_a".to_string());

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // tenant_a should be disabled
    assert!(!manager.is_durable_writes_enabled("tenant_a"));

    // tenant_b should be enabled
    assert!(manager.is_durable_writes_enabled("tenant_b"));
}

#[test]
fn test_denylist_overrides_allowlist() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 100;

    // Add tenant_a to both allowlist and denylist
    flags
        .durable_writes
        .enabled_tenants
        .insert("tenant_a".to_string());
    flags
        .durable_writes
        .disabled_tenants
        .insert("tenant_a".to_string());

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Denylist should take precedence
    assert!(!manager.is_durable_writes_enabled("tenant_a"));
}

#[test]
fn test_emergency_disable() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.enabled = true;
    flags.durable_writes.rollout_percentage = 100;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Should be enabled initially
    assert!(manager.is_durable_writes_enabled("tenant_a"));

    // Emergency disable
    manager.emergency_disable_all();

    // Should now be disabled
    assert!(!manager.is_durable_writes_enabled("tenant_a"));
}

#[test]
fn test_emergency_enable() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.emergency_disable = true; // Set emergency disable

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Should be disabled initially due to emergency disable
    assert!(!manager.is_durable_writes_enabled("tenant_a"));

    // Emergency enable (clears emergency_disable flag)
    manager.emergency_enable_all();

    // Should now be enabled (emergency_disable is cleared)
    assert!(manager.is_durable_writes_enabled("tenant_a"));
}

#[test]
fn test_runtime_flag_updates() {
    let manager = FeatureFlagManager::new();

    // Initially default flags (100% rollout)
    assert!(manager.is_durable_writes_enabled("tenant_a"));

    // Update to 0% rollout
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 0;
    manager.update_flags(flags);

    // Should now be disabled
    assert!(!manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", "event_123"));

    // Update back to 100% rollout
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 100;
    manager.update_flags(flags);

    // Should be enabled again
    assert!(manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", "event_123"));
}

#[tokio::test]
async fn test_concurrent_flag_evaluation() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 50;

    let manager = Arc::new(FeatureFlagManager::new());
    manager.update_flags(flags);

    // Spawn 10 concurrent tasks evaluating flags
    let mut handles = vec![];
    for i in 0..10 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move {
            for j in 0..1000 {
                let event_id = format!("event_{}_{}", i, j);
                let _ = mgr.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id);
            }
        });
        handles.push(handle);
    }

    // All tasks should complete without panicking
    for handle in handles {
        handle.await.unwrap();
    }
}

#[tokio::test]
async fn test_concurrent_flag_updates() {
    let manager = Arc::new(FeatureFlagManager::new());

    // Spawn 5 concurrent tasks updating flags
    let mut handles = vec![];
    for i in 0..5 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..100 {
                let mut flags = FeatureFlags::default();
                flags.durable_writes.rollout_percentage = (i * 20) as u8;
                mgr.update_flags(flags);
            }
        });
        handles.push(handle);
    }

    // All updates should complete without panicking
    for handle in handles {
        handle.await.unwrap();
    }
}

#[test]
fn test_hash_consistency() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 50;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Same event ID should always get same result
    let event_id = "test_event_123";
    let result1 = manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", event_id);
    let result2 = manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", event_id);
    let result3 = manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", event_id);

    assert_eq!(result1, result2);
    assert_eq!(result2, result3);
}

#[test]
fn test_high_volume_evaluation() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 50;

    let manager = Arc::new(FeatureFlagManager::new());
    manager.update_flags(flags);

    // Evaluate 100,000 events
    let start = std::time::Instant::now();

    for i in 0..100000 {
        let event_id = format!("event_{}", i);
        let _ = manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id);
    }

    let duration = start.elapsed();

    // Should complete in less than 1 second for 100k evaluations
    assert!(
        duration.as_secs() < 1,
        "Evaluation too slow: {:?}",
        duration
    );
}

#[test]
fn test_large_tenant_allowlist() {
    let mut flags = FeatureFlags::default();

    // Add 1000 tenants to allowlist
    for i in 0..1000 {
        flags
            .durable_writes
            .enabled_tenants
            .insert(format!("tenant_{}", i));
    }

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // All 1000 tenants should be enabled
    for i in 0..1000 {
        let tenant_id = format!("tenant_{}", i);
        assert!(manager.is_durable_writes_enabled(&tenant_id));
    }

    // tenant_1000 should be disabled (not in allowlist)
    assert!(!manager.is_durable_writes_enabled("tenant_1000"));
}

#[test]
fn test_edge_case_empty_tenant_id() {
    let flags = FeatureFlags::default();
    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Empty tenant ID should still work
    assert!(manager.is_durable_writes_enabled(""));
}

#[test]
fn test_edge_case_empty_event_id() {
    let flags = FeatureFlags::default();
    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Empty event ID should still work
    assert!(manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", ""));
}

#[test]
fn test_edge_case_very_long_strings() {
    let flags = FeatureFlags::default();
    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Very long tenant ID (10KB)
    let long_tenant = "a".repeat(10000);
    assert!(manager.is_durable_writes_enabled(&long_tenant));

    // Very long event ID (10KB)
    let long_event = "b".repeat(10000);
    assert!(manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &long_event));
}

#[test]
fn test_edge_case_special_characters() {
    let flags = FeatureFlags::default();
    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Special characters in tenant ID
    let special_tenant = "tenant!@#$%^&*()_+-={}[]|\\:;\"'<>,.?/";
    assert!(manager.is_durable_writes_enabled(special_tenant));

    // Special characters in event ID
    let special_event = "event!@#$%^&*()_+-={}[]|\\:;\"'<>,.?/";
    assert!(manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", special_event));
}

#[test]
fn test_rollout_stats() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 50;

    let stats = flags.get_rollout_stats();

    // Stats should reflect current config
    assert_eq!(stats.durable_writes_pct, 50);
    assert!(stats.startup_recovery_enabled);
    assert!(stats.metrics_enabled);
    assert!(stats.tracing_enabled);
}

#[test]
fn test_circuit_breaker_flag() {
    let flags = FeatureFlags::default();

    assert!(flags.is_circuit_breaker_enabled("tenant_a"));
}

#[test]
fn test_startup_recovery_flag() {
    let flags = FeatureFlags::default();

    assert!(flags.is_startup_recovery_enabled());
}

#[test]
fn test_acknowledgment_tracking_flag() {
    let flags = FeatureFlags::default();

    assert!(flags.is_acknowledgment_tracking_enabled("tenant_a"));
}

#[test]
fn test_metrics_flag() {
    let flags = FeatureFlags::default();

    assert!(flags.is_metrics_enabled());
}

#[test]
fn test_tracing_flag() {
    let flags = FeatureFlags::default();

    assert!(flags.is_tracing_enabled());
}

#[test]
fn test_disabled_feature_overrides_rollout() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.enabled = false; // Globally disabled
    flags.durable_writes.rollout_percentage = 100; // 100% rollout

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Should be disabled despite 100% rollout
    assert!(!manager.is_durable_writes_enabled("tenant_a"));
}

#[test]
fn test_zero_percent_rollout() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 0;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Should be disabled for all events
    for i in 0..100 {
        let event_id = format!("event_{}", i);
        assert!(!manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id));
    }
}

#[test]
fn test_hundred_percent_rollout() {
    let mut flags = FeatureFlags::default();
    flags.durable_writes.rollout_percentage = 100;

    let manager = FeatureFlagManager::new();
    manager.update_flags(flags);

    // Should be enabled for all events
    for i in 0..100 {
        let event_id = format!("event_{}", i);
        assert!(manager.is_durable_writes_enabled_for_event("tenant_a", "dataset_1", &event_id));
    }
}
