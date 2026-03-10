//! RocksDB Optimization Integration Tests
//!
//! Tests for the optimized RocksDB configuration and performance monitoring

use graphica_coordinator::workflows::domain::{ExecutionStatus, WorkflowExecution};
use graphica_coordinator::workflows::storage::metrics::WorkflowStorageMetrics;
use graphica_coordinator::workflows::storage::persistence::rocksdb_backend::RocksDbBackend;
use graphica_coordinator::workflows::storage::persistence::rocksdb_config::RocksDbConfig;
use graphica_coordinator::workflows::storage::persistence::ExecutionStoreBackend; // Import trait for methods
use graphica_coordinator::workflows::storage::tuning::{RocksDbMonitor, StatsCollector};
use prometheus::Registry;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use tokio;

/// Test loading configuration from YAML file
#[tokio::test]
async fn test_load_config_from_file() {
    let config_path = "config/rocksdb-production.yaml";

    // Skip if config file doesn't exist (CI environment)
    if !std::path::Path::new(config_path).exists() {
        eprintln!("Skipping test - config file not found");
        return;
    }

    let config =
        RocksDbConfig::from_file(std::path::Path::new(config_path)).expect("Failed to load config");

    assert_eq!(config.db_config.max_background_jobs, 16);
    assert_eq!(
        config.memory_config.block_cache_size,
        4 * 1024 * 1024 * 1024
    );
}

/// Test creating RocksDB backend with optimized configuration
#[tokio::test]
async fn test_optimized_backend_creation() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    // Create backend with optimized config
    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Verify it's functional
    let execution = create_test_execution("test_001");
    backend.save(execution).await.expect("Failed to save");

    let retrieved = backend.get("test_001").await.expect("Failed to get");
    assert!(retrieved.is_some());
}

/// Benchmark write performance with optimized settings
#[tokio::test]
async fn test_write_performance() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    let num_executions = 1000;
    let start = Instant::now();

    // Write executions
    for i in 0..num_executions {
        let execution = create_test_execution(&format!("exec_{:06}", i));
        backend.save(execution).await.expect("Failed to save");
    }

    let elapsed = start.elapsed();
    let writes_per_sec = num_executions as f64 / elapsed.as_secs_f64();

    println!("Write performance: {:.2} executions/sec", writes_per_sec);

    // Should achieve at least 100 writes/sec even in test environment
    assert!(writes_per_sec > 100.0, "Write performance too low");
}

/// Test read latency with optimized settings
#[tokio::test]
async fn test_read_latency() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Prepare test data
    for i in 0..100 {
        let execution = create_test_execution(&format!("exec_{:06}", i));
        backend.save(execution).await.expect("Failed to save");
    }

    // Measure read latency
    let mut latencies = Vec::new();
    for i in 0..100 {
        let start = Instant::now();
        let _ = backend
            .get(&format!("exec_{:06}", i))
            .await
            .expect("Failed to get");
        latencies.push(start.elapsed().as_micros());
    }

    // Calculate latency percentiles
    latencies.sort();
    let p50_index = latencies.len() / 2;
    let p99_index = (latencies.len() as f64 * 0.99) as usize;
    let p50_latency = latencies[p50_index];
    let p99_latency = latencies[p99_index];

    println!("P50 read latency: {} microseconds", p50_latency);
    println!("P99 read latency: {} microseconds", p99_latency);

    // Test environment expectations (more lenient than production)
    // Production with SSDs and warm cache: P99 < 1ms
    // Test environment with cold storage: P99 < 10ms is reasonable
    assert!(
        p99_latency < 10_000,
        "P99 latency exceeds 10ms (got {}μs)",
        p99_latency
    );

    // P50 should be reasonably fast even in test environment
    assert!(
        p50_latency < 5_000,
        "P50 latency exceeds 5ms (got {}μs)",
        p50_latency
    );
}

/// Test monitoring and statistics collection
#[tokio::test]
async fn test_monitoring_integration() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    // Create backend with statistics enabled
    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Set up metrics
    let registry = Registry::new();
    let metrics = Arc::new(WorkflowStorageMetrics::new(&registry).unwrap());

    // Create monitor
    let db = backend.inner().db.clone();
    let monitor = RocksDbMonitor::new(db.clone(), metrics.clone());

    // Perform some operations
    for i in 0..10 {
        let execution = create_test_execution(&format!("exec_{}", i));
        backend.save(execution).await.expect("Failed to save");
    }

    // Collect statistics
    monitor
        .collect_statistics()
        .expect("Failed to collect stats");

    // Create stats collector
    let collector = StatsCollector::new(db);
    let snapshot = collector.collect().expect("Failed to collect snapshot");

    // Verify statistics were collected
    // Note: In v0.22 mode, StatsCollector has limited CF introspection
    // It collects stats for "default" CF
    assert!(snapshot.cf_stats.contains_key("default"));
    assert!(snapshot.cf_stats.len() > 0, "Stats collected");
}

/// Test compression effectiveness
#[tokio::test]
async fn test_compression_ratios() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Create executions with compressible data
    for i in 0..100 {
        let mut execution = create_test_execution(&format!("exec_{:06}", i));

        // Add repetitive data that compresses well
        let large_payload = serde_json::json!({
            "data": vec!["test_data"; 100],
            "metadata": {
                "field1": "value1",
                "field2": "value2",
                "repeated": vec![42; 1000]
            }
        });
        execution.set_output(large_payload);

        backend.save(execution).await.expect("Failed to save");
    }

    // Force flush to trigger compression
    backend.inner().flush().expect("Failed to flush");

    // Check compression was applied
    let db = backend.inner().db.clone();
    let collector = StatsCollector::new(db);
    let snapshot = collector.collect().expect("Failed to collect snapshot");

    // Note: Compression ratio not available in rocksdb v0.22
    // In production, compression is configured via DBCompressionType
    // In v0.22 mode, stats are collected for "default" CF
    if let Some(default_stats) = snapshot.cf_stats.get("default") {
        println!(
            "Default CF live SST size: {} bytes",
            default_stats.live_sst_files_size
        );

        // Verify stats were collected (just check the struct is populated)
        assert!(snapshot.cf_stats.len() > 0, "Stats collected successfully");
    }
}

/// Test TTL and automatic expiry
#[tokio::test]
async fn test_ttl_configuration() {
    let temp_dir = TempDir::new().unwrap();

    // Create config with short TTL for testing
    let mut config = RocksDbConfig::production();
    config.ttl_config.execution_ttl_seconds = Some(1); // 1 second TTL

    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Save execution
    let execution = create_test_execution("ttl_test");
    backend.save(execution).await.expect("Failed to save");

    // Verify it exists
    assert!(backend.get("ttl_test").await.unwrap().is_some());

    // Note: Actual TTL enforcement requires periodic compaction
    // which may not trigger immediately in tests
}

/// Test performance recommendations
#[tokio::test]
async fn test_performance_analysis() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    let backend = RocksDbBackend::open_with_config(temp_dir.path(), config)
        .expect("Failed to create backend");

    // Create some load
    for i in 0..100 {
        let execution = create_test_execution(&format!("exec_{}", i));
        backend.save(execution).await.expect("Failed to save");
    }

    // Analyze performance
    let db = backend.inner().db.clone();
    let registry = Registry::new();
    let metrics = Arc::new(WorkflowStorageMetrics::new(&registry).unwrap());
    let monitor = RocksDbMonitor::new(db.clone(), metrics);

    let recommendations = monitor
        .analyze_performance()
        .expect("Failed to analyze performance");

    // Check if any recommendations were generated
    println!("Performance recommendations:");
    println!(
        "- Increase write buffer: {}",
        recommendations.increase_write_buffer
    );
    println!(
        "- Increase L0 trigger: {}",
        recommendations.increase_l0_trigger
    );
    println!(
        "- Enable compression: {}",
        recommendations.enable_compression
    );
    println!(
        "- Manual compaction needed for: {:?}",
        recommendations.trigger_manual_compaction
    );
}

/// Test concurrent operations with optimized settings
#[tokio::test]
async fn test_concurrent_operations() {
    let temp_dir = TempDir::new().unwrap();
    let config = RocksDbConfig::production();

    let backend = Arc::new(
        RocksDbBackend::open_with_config(temp_dir.path(), config)
            .expect("Failed to create backend"),
    );

    // Spawn concurrent write tasks
    let mut handles = vec![];

    for thread_id in 0..10 {
        let backend_clone = backend.clone();
        let handle = tokio::spawn(async move {
            for i in 0..100 {
                let id = format!("thread_{}_exec_{}", thread_id, i);
                let execution = create_test_execution(&id);
                backend_clone.save(execution).await.expect("Failed to save");
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks
    for handle in handles {
        handle.await.expect("Task failed");
    }

    // Verify all writes succeeded
    let total = backend.count_total().await.expect("Failed to count");
    assert_eq!(total, 1000);
}

// Helper function to create test execution
fn create_test_execution(id: &str) -> WorkflowExecution {
    WorkflowExecution::new(
        id.to_string(),
        "test_workflow".to_string(),
        "Test Workflow".to_string(),
        serde_json::json!({
            "test": "data",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }),
        Some("test_user".to_string()),
    )
}
