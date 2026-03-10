//! Integration Test for Cache Metrics API
//!
//! Tests the cache metrics endpoint:
//! - GET /api/v1/resolved-entities/cache/metrics
//!
//! Validates metrics tracking and API response format.

use chrono::Utc;
use graphica_coordinator::api::resolved_entity_cache::{
    CachedFieldValue, CachedResolvedEntity, ResolvedEntityCache,
};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_cache_metrics_tracking() {
    println!("\n=== Cache Metrics Tracking Test ===\n");

    let cache = ResolvedEntityCache::new();

    // Initial state - no operations
    let metrics = cache.metrics();
    assert_eq!(metrics.hits.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(metrics.misses.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert_eq!(
        metrics
            .insertions
            .load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(metrics.size(), 0);
    println!("✓ Initial metrics are zero");

    // Insert a record
    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        CachedFieldValue {
            value: json!("test@example.com"),
            confidence: 0.95,
            resolved_at: Utc::now(),
        },
    );

    let record = CachedResolvedEntity {
        entity_id: "test_entity".to_string(),
        fields,
        overall_confidence: 0.95,
        conflict_count: 0,
        requires_review: false,
        created_at: Utc::now(),
        cached_at: Utc::now(),
        access_count: 0,
        source_count: 2,
    };

    cache.insert(record);

    // Check insertion metrics
    let metrics = cache.metrics();
    assert_eq!(
        metrics
            .insertions
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    assert_eq!(metrics.size(), 1);
    println!(
        "✓ Insertion tracked: {} insertions, size: {}",
        metrics
            .insertions
            .load(std::sync::atomic::Ordering::Relaxed),
        metrics.size()
    );

    // Cache hit
    let _result = cache.get("test_entity");
    let metrics = cache.metrics();
    assert_eq!(metrics.hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(metrics.misses.load(std::sync::atomic::Ordering::Relaxed), 0);
    println!(
        "✓ Cache hit tracked: {} hits",
        metrics.hits.load(std::sync::atomic::Ordering::Relaxed)
    );

    // Cache miss
    let _result = cache.get("nonexistent");
    let metrics = cache.metrics();
    assert_eq!(metrics.hits.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(metrics.misses.load(std::sync::atomic::Ordering::Relaxed), 1);
    println!(
        "✓ Cache miss tracked: {} misses",
        metrics.misses.load(std::sync::atomic::Ordering::Relaxed)
    );

    // Calculate hit rate
    let hit_rate = metrics.hit_rate();
    assert!((hit_rate - 0.5).abs() < 0.01); // 1 hit, 1 miss = 50%
    println!("✓ Hit rate calculated correctly: {:.2}%", hit_rate * 100.0);

    // Multiple cache hits
    for _ in 0..10 {
        cache.get("test_entity");
    }

    let metrics = cache.metrics();
    assert_eq!(metrics.hits.load(std::sync::atomic::Ordering::Relaxed), 11); // 1 + 10
    let hit_rate = metrics.hit_rate();
    assert!((hit_rate - 0.916).abs() < 0.01); // 11/(11+1) = 91.6%
    println!(
        "✓ Multiple hits tracked: {} total hits, {:.2}% hit rate",
        metrics.hits.load(std::sync::atomic::Ordering::Relaxed),
        hit_rate * 100.0
    );

    println!("\n=== Cache Metrics Tracking Test PASSED ===\n");
}

#[test]
fn test_cache_metrics_eviction_tracking() {
    println!("\n=== Cache Eviction Metrics Test ===\n");

    // Create small cache to trigger evictions
    use graphica_coordinator::api::resolved_entity_cache::CacheConfig;

    let config = CacheConfig {
        max_size: 3,
        ttl_seconds: 300,
        enable_cleanup: false, // Disable background cleanup for test
        cleanup_interval_seconds: 60,
    };

    let cache = ResolvedEntityCache::with_config(config);

    // Insert 5 records (max is 3, so 2 should be evicted)
    for i in 0..5 {
        let mut fields = HashMap::new();
        fields.insert(
            "email".to_string(),
            CachedFieldValue {
                value: json!(format!("test{}@example.com", i)),
                confidence: 0.9,
                resolved_at: Utc::now(),
            },
        );

        let record = CachedResolvedEntity {
            entity_id: format!("entity_{}", i),
            fields,
            overall_confidence: 0.9,
            conflict_count: 0,
            requires_review: false,
            created_at: Utc::now(),
            cached_at: Utc::now(),
            access_count: 0,
            source_count: 1,
        };

        cache.insert(record);

        // Small delay between insertions
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Check eviction metrics
    let metrics = cache.metrics();
    let size_evictions = metrics
        .size_evictions
        .load(std::sync::atomic::Ordering::Relaxed);
    assert_eq!(size_evictions, 2, "Should have 2 size-based evictions");
    assert_eq!(metrics.size(), 3, "Cache size should be at max (3)");

    println!(
        "✓ Size-based evictions tracked: {} evictions",
        size_evictions
    );
    println!("✓ Cache size maintained at max: {}", metrics.size());

    println!("\n=== Cache Eviction Metrics Test PASSED ===\n");
}

#[test]
fn test_cache_metrics_api_response_format() {
    println!("\n=== Cache Metrics API Response Format Test ===\n");

    // This test validates the metrics response structure
    // In a real integration test, this would call the actual API endpoint
    // For now, we're testing the data structure that would be returned

    use graphica_coordinator::api::field_lineage::handlers::CacheMetricsResponse;

    let cache = ResolvedEntityCache::new();

    // Perform some operations
    let mut fields = HashMap::new();
    fields.insert(
        "email".to_string(),
        CachedFieldValue {
            value: json!("test@example.com"),
            confidence: 0.95,
            resolved_at: Utc::now(),
        },
    );

    let record = CachedResolvedEntity {
        entity_id: "test_entity".to_string(),
        fields,
        overall_confidence: 0.95,
        conflict_count: 0,
        requires_review: false,
        created_at: Utc::now(),
        cached_at: Utc::now(),
        access_count: 0,
        source_count: 2,
    };

    cache.insert(record);
    cache.get("test_entity"); // Hit
    cache.get("nonexistent"); // Miss

    // Build response (simulating what the API endpoint would return)
    let metrics = cache.metrics();
    let response = CacheMetricsResponse {
        hits: metrics.hits.load(std::sync::atomic::Ordering::Relaxed),
        misses: metrics.misses.load(std::sync::atomic::Ordering::Relaxed),
        hit_rate: metrics.hit_rate(),
        insertions: metrics
            .insertions
            .load(std::sync::atomic::Ordering::Relaxed),
        ttl_evictions: metrics
            .ttl_evictions
            .load(std::sync::atomic::Ordering::Relaxed),
        size_evictions: metrics
            .size_evictions
            .load(std::sync::atomic::Ordering::Relaxed),
        current_size: metrics.size(),
        max_size: 10_000,
    };

    // Verify response structure
    assert_eq!(response.hits, 1);
    assert_eq!(response.misses, 1);
    assert!((response.hit_rate - 0.5).abs() < 0.01);
    assert_eq!(response.insertions, 1);
    assert_eq!(response.current_size, 1);
    assert_eq!(response.max_size, 10_000);

    println!("✓ Response structure validated:");
    println!("  - hits: {}", response.hits);
    println!("  - misses: {}", response.misses);
    println!("  - hit_rate: {:.2}%", response.hit_rate * 100.0);
    println!("  - insertions: {}", response.insertions);
    println!("  - ttl_evictions: {}", response.ttl_evictions);
    println!("  - size_evictions: {}", response.size_evictions);
    println!("  - current_size: {}", response.current_size);
    println!("  - max_size: {}", response.max_size);

    // Test JSON serialization
    let json_response = serde_json::to_value(&response).expect("Should serialize to JSON");
    assert!(json_response.is_object());
    assert!(json_response.get("hits").is_some());
    assert!(json_response.get("hit_rate").is_some());

    println!("✓ JSON serialization successful:");
    println!("{}", serde_json::to_string_pretty(&json_response).unwrap());

    println!("\n=== Cache Metrics API Response Format Test PASSED ===\n");
}

#[test]
fn test_cache_metrics_concurrent_access() {
    println!("\n=== Cache Metrics Concurrent Access Test ===\n");

    let cache = std::sync::Arc::new(ResolvedEntityCache::new());

    // Insert initial records
    for i in 0..10 {
        let mut fields = HashMap::new();
        fields.insert(
            "email".to_string(),
            CachedFieldValue {
                value: json!(format!("test{}@example.com", i)),
                confidence: 0.9,
                resolved_at: Utc::now(),
            },
        );

        let record = CachedResolvedEntity {
            entity_id: format!("entity_{}", i),
            fields,
            overall_confidence: 0.9,
            conflict_count: 0,
            requires_review: false,
            created_at: Utc::now(),
            cached_at: Utc::now(),
            access_count: 0,
            source_count: 1,
        };

        cache.insert(record);
    }

    // Spawn multiple threads to access cache concurrently
    let mut handles = vec![];
    for _ in 0..10 {
        let cache_clone = cache.clone();
        let handle = std::thread::spawn(move || {
            for i in 0..100 {
                // Mix of hits and misses
                if i % 2 == 0 {
                    cache_clone.get(&format!("entity_{}", i % 10)); // Hit
                } else {
                    cache_clone.get(&format!("nonexistent_{}", i)); // Miss
                }
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    // Verify metrics
    let metrics = cache.metrics();
    let total_ops = metrics.hits.load(std::sync::atomic::Ordering::Relaxed)
        + metrics.misses.load(std::sync::atomic::Ordering::Relaxed);

    assert_eq!(
        total_ops, 1000,
        "Should have 1000 total cache operations (10 threads × 100 ops)"
    );
    assert_eq!(
        metrics.hits.load(std::sync::atomic::Ordering::Relaxed),
        500,
        "Should have 500 hits"
    );
    assert_eq!(
        metrics.misses.load(std::sync::atomic::Ordering::Relaxed),
        500,
        "Should have 500 misses"
    );

    println!("✓ Concurrent access metrics:");
    println!("  - Total operations: {}", total_ops);
    println!(
        "  - Hits: {}",
        metrics.hits.load(std::sync::atomic::Ordering::Relaxed)
    );
    println!(
        "  - Misses: {}",
        metrics.misses.load(std::sync::atomic::Ordering::Relaxed)
    );
    println!("  - Hit rate: {:.2}%", metrics.hit_rate() * 100.0);

    println!("\n=== Cache Metrics Concurrent Access Test PASSED ===\n");
}
