//! Example: Using Optimized RocksDB Configuration
//!
//! Demonstrates how to use the optimized RocksDB backend for workflow storage
//! with production-grade configuration, monitoring, and performance tuning.

use graphica_coordinator::workflows::domain::WorkflowExecution;
use graphica_coordinator::workflows::storage::metrics::WorkflowStorageMetrics;
use graphica_coordinator::workflows::storage::persistence::{
    ExecutionStoreBackend, RocksDbBackend, RocksDbConfig,
};
use graphica_coordinator::workflows::storage::tuning::{RocksDbMonitor, StatsCollector};
use prometheus::Registry;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use tokio;
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting optimized RocksDB workflow storage example");

    // Create temporary directory for the database
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path();

    info!("Database path: {:?}", db_path);

    // Load configuration
    let config = if std::path::Path::new("config/rocksdb-production.yaml").exists() {
        info!("Loading configuration from file");
        RocksDbConfig::from_file(std::path::Path::new("config/rocksdb-production.yaml"))?
    } else {
        info!("Using default production configuration");
        RocksDbConfig::production()
    };

    // Display configuration summary
    info!("Configuration summary:");
    info!(
        "  Max background jobs: {}",
        config.db_config.max_background_jobs
    );
    info!(
        "  Block cache size: {} MB",
        config.memory_config.block_cache_size / (1024 * 1024)
    );
    info!(
        "  Write buffer budget: {} MB",
        config.memory_config.write_buffer_budget / (1024 * 1024)
    );
    info!(
        "  Executions compression: {:?}",
        config.cf_configs.executions.compression
    );

    // Create optimized backend
    let backend = Arc::new(RocksDbBackend::open_with_config(db_path, config)?);

    // Set up metrics
    let registry = Registry::new();
    let metrics = Arc::new(WorkflowStorageMetrics::new(&registry)?);

    // Spawn monitoring task
    let db = backend.inner().db.clone();
    let monitor_handle = {
        let db_clone = db.clone();
        let metrics_clone = metrics.clone();
        tokio::spawn(async move {
            let monitor = RocksDbMonitor::new(db_clone, metrics_clone).with_interval(5);
            monitor.start_monitoring().await;
        })
    };

    // Example 1: Write Performance Test
    info!("\n=== Write Performance Test ===");
    let num_writes = 1000;
    let start = Instant::now();

    for i in 0..num_writes {
        let execution = create_test_execution(&format!("exec_{:06}", i));
        backend.save(execution).await?;

        if (i + 1) % 100 == 0 {
            info!("Saved {} executions", i + 1);
        }
    }

    let write_duration = start.elapsed();
    let writes_per_sec = num_writes as f64 / write_duration.as_secs_f64();
    info!("Write performance: {:.2} executions/sec", writes_per_sec);
    info!(
        "Average write latency: {:.2} ms",
        write_duration.as_millis() as f64 / num_writes as f64
    );

    // Example 2: Read Performance Test
    info!("\n=== Read Performance Test ===");
    let num_reads = 1000;
    let mut read_latencies = Vec::new();

    for i in 0..num_reads {
        let id = format!("exec_{:06}", i % num_writes);
        let start = Instant::now();
        let _ = backend.get(&id).await?;
        read_latencies.push(start.elapsed());
    }

    // Calculate statistics
    read_latencies.sort();
    let p50_index = read_latencies.len() / 2;
    let p99_index = (read_latencies.len() as f64 * 0.99) as usize;

    info!("Read performance:");
    info!("  P50 latency: {:?}", read_latencies[p50_index]);
    info!("  P99 latency: {:?}", read_latencies[p99_index]);

    // Example 3: Collect and Display Statistics
    info!("\n=== Database Statistics ===");
    let collector = StatsCollector::new(db.clone());
    let snapshot = collector.collect()?;

    info!("Block cache statistics:");
    info!("  Cache hits: {}", snapshot.block_cache_stats.cache_hits);
    info!(
        "  Cache misses: {}",
        snapshot.block_cache_stats.cache_misses
    );
    info!(
        "  Hit rate: {:.2}%",
        snapshot.block_cache_stats.cache_hit_rate * 100.0
    );

    info!("\nCompaction statistics:");
    info!(
        "  Bytes read: {} MB",
        snapshot.compaction_stats.compaction_bytes_read / (1024 * 1024)
    );
    info!(
        "  Bytes written: {} MB",
        snapshot.compaction_stats.compaction_bytes_written / (1024 * 1024)
    );
    info!(
        "  Write amplification: {:.2}x",
        snapshot.compaction_stats.write_amplification
    );

    info!("\nColumn family statistics:");
    for (cf_name, cf_stats) in &snapshot.cf_stats {
        info!("  {}:", cf_name);
        info!("    Estimated keys: {}", cf_stats.estimate_num_keys);
        info!(
            "    Live SST size: {} MB",
            cf_stats.live_sst_files_size / (1024 * 1024)
        );
        info!(
            "    Immutable memtables: {}",
            cf_stats.num_immutable_mem_tables
        );
        info!(
            "    Running compactions: {}",
            cf_stats.num_running_compactions
        );
    }

    // Example 4: Performance Analysis
    info!("\n=== Performance Analysis ===");
    let monitor = RocksDbMonitor::new(db.clone(), metrics.clone());
    let recommendations = monitor.analyze_performance()?;

    info!("Performance recommendations:");
    if recommendations.increase_write_buffer {
        info!("  - Consider increasing write buffer size");
    }
    if recommendations.increase_l0_trigger {
        info!("  - Consider increasing L0 compaction trigger");
    }
    if recommendations.enable_compression {
        info!("  - Consider enabling compression for better space efficiency");
    }
    if !recommendations.trigger_manual_compaction.is_empty() {
        info!(
            "  - Manual compaction recommended for: {:?}",
            recommendations.trigger_manual_compaction
        );
    }

    // Example 5: Generate Performance Report
    info!("\n=== Performance Report ===");
    let summary = collector.get_summary()?;
    info!("\n{}", summary);

    // Also analyze performance and get recommendations
    let recommendations = collector.analyze_performance()?;
    if !recommendations.is_empty() {
        info!("\nOptimization recommendations:");
        for recommendation in &recommendations {
            info!("  - {}", recommendation);
        }
    }

    // Example 6: Concurrent Operations Test
    info!("\n=== Concurrent Operations Test ===");
    let backend_clone = backend.clone();
    let mut handles = vec![];

    for thread_id in 0..5 {
        let backend = backend_clone.clone();
        let handle = tokio::spawn(async move {
            for i in 0..20 {
                let id = format!("concurrent_{}_{}", thread_id, i);
                let execution = create_test_execution(&id);
                backend.save(execution).await.expect("Failed to save");
            }
        });
        handles.push(handle);
    }

    // Wait for all concurrent operations
    for handle in handles {
        handle.await?;
    }

    info!("Concurrent operations completed successfully");

    // Final statistics
    info!("\n=== Final Statistics ===");
    let total = backend.count_total().await?;
    info!("Total executions stored: {}", total);

    // Memory usage
    if let Ok(compaction_summary) = monitor.get_compaction_summary() {
        info!(
            "Pending compaction: {} MB",
            compaction_summary.total_pending_bytes / (1024 * 1024)
        );
        info!(
            "Running compactions: {}",
            compaction_summary.running_compactions
        );
    }

    // Clean shutdown
    info!("\nShutting down...");
    monitor_handle.abort();

    Ok(())
}

/// Create a test workflow execution
fn create_test_execution(id: &str) -> WorkflowExecution {
    WorkflowExecution::new(
        id.to_string(),
        "example_workflow".to_string(),
        "Example Workflow".to_string(),
        serde_json::json!({
            "test_data": "This is test data that should compress well",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "metadata": {
                "field1": "value1",
                "field2": "value2",
                "field3": "value3",
                "array": vec![1, 2, 3, 4, 5],
                "nested": {
                    "inner_field": "inner_value",
                    "numbers": vec![10, 20, 30, 40, 50]
                }
            }
        }),
        Some("example_user".to_string()),
    )
}
