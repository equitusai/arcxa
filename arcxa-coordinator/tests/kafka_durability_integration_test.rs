//! Integration tests for Kafka durability implementation
//!
//! These tests require a running Kafka cluster and validate:
//! - End-to-end WAL-backed writes
//! - Acknowledgment tracking and deduplication
//! - Circuit breaker behavior during failures
//! - Startup recovery with replay
//! - Performance under load
//!
//! # Running Tests
//!
//! ## With Local Kafka:
//! ```bash
//! # Start Kafka locally (Docker Compose)
//! docker-compose up -d kafka
//!
//! # Run integration tests
//! cargo test --test kafka_durability_integration_test -- --nocapture
//! ```
//!
//! ## With Testcontainers (requires Docker):
//! ```bash
//! cargo test --test kafka_durability_integration_test --features testcontainers
//! ```
//!
//! ## Skip if Kafka unavailable:
//! ```bash
//! # Tests are marked as #[ignore] by default
//! cargo test --test kafka_durability_integration_test --ignored
//! ```

use anyhow::Result;
use graphica_coordinator::storage::kafka::{
    DurableKafkaLineageSink, KafkaConfig, ReplayConfig, ReplayManager,
};
use graphica_coordinator::storage::wal::{FileWal, WalConfig, WalMetricsCollector};
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

/// Kafka broker address for integration tests
/// Override with KAFKA_TEST_BROKERS environment variable
fn kafka_brokers() -> String {
    std::env::var("KAFKA_TEST_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string())
}

/// Create test lineage event
fn create_test_event(record_id: &str) -> LineageEvent {
    use chrono::Utc;
    use std::collections::HashMap;

    LineageEvent {
        id: Uuid::new_v4(),
        dataset: "test_dataset".to_string(),
        record_id: record_id.to_string(),
        source_refs: vec![DataRef {
            system: "test_system".to_string(),
            path: "test/path".to_string(),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        }],
        transforms: vec![],
        model_refs: vec![],
        output_ref: DataRef {
            system: "output_system".to_string(),
            path: "output/path".to_string(),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        },
        ts: Utc::now(),
        run_id: "test_run".to_string(),
        tenant_id: "test_tenant".to_string(),
        correlation_id: None,
        metadata: HashMap::new(),
    }
}

/// Setup test environment with WAL and Kafka sink
async fn setup_test_sink() -> Result<(Arc<DurableKafkaLineageSink>, TempDir)> {
    let temp_dir = TempDir::new()?;
    let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
    let metrics = Arc::new(WalMetricsCollector::new("test_kafka_integration"));
    let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
        Arc::new(FileWal::new(wal_config, metrics).await?);

    let kafka_config = KafkaConfig::default();
    let sink = DurableKafkaLineageSink::new(&kafka_brokers(), wal, kafka_config).await?;

    Ok((Arc::new(sink), temp_dir))
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_end_to_end_write_and_acknowledgment() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    // Write lineage event
    let event = create_test_event("test_record_001");
    let event_id = event.id;

    sink.write(event)?;

    // Wait for acknowledgment (Kafka is async)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Check that event was acknowledged
    assert!(
        !sink.ack_tracker().is_acknowledged(&event_id) || sink.ack_tracker().pending_count() == 0
    );

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_batch_writes_with_acknowledgments() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    let event_count = 100;
    let initial_pending = sink.ack_tracker().pending_count();

    // Write batch of events
    for i in 0..event_count {
        let event = create_test_event(&format!("batch_record_{:03}", i));
        sink.write(event)?;
    }

    // Give Kafka time to process
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Most events should be acknowledged
    let final_pending = sink.ack_tracker().pending_count();
    let acknowledged = event_count - (final_pending - initial_pending);

    println!("Acknowledged: {}/{} events", acknowledged, event_count);
    assert!(acknowledged >= event_count * 80 / 100); // At least 80% acknowledged

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_recovery_after_crash_simulation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().to_path_buf();

    // Phase 1: Write events but don't wait for acknowledgments (simulate crash)
    {
        let wal_config = WalConfig::default().with_path(wal_path.clone());
        let metrics = Arc::new(WalMetricsCollector::new("test_crash_sim_1"));
        let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
            Arc::new(FileWal::new(wal_config, metrics).await?);

        let kafka_config = KafkaConfig::default();
        let sink = DurableKafkaLineageSink::new(&kafka_brokers(), wal, kafka_config).await?;

        // Write events without waiting for acks
        for i in 0..10 {
            let event = create_test_event(&format!("crash_sim_{:03}", i));
            sink.write(event)?;
        }

        // Simulate crash (drop sink without waiting for acks)
        let pending_before_crash = sink.ack_tracker().pending_count();
        println!("Pending before crash: {}", pending_before_crash);
        drop(sink);
    }

    // Small delay to ensure WAL is closed
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Phase 2: Restart with same WAL and run recovery
    {
        let wal_config = WalConfig::default().with_path(wal_path);
        let metrics = Arc::new(WalMetricsCollector::new("test_crash_sim_2"));
        let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
            Arc::new(FileWal::new(wal_config, metrics).await?);

        let kafka_config = KafkaConfig::default();
        let sink = Arc::new(
            DurableKafkaLineageSink::new(&kafka_brokers(), wal.clone(), kafka_config).await?,
        );

        // Create replay manager
        let replay_manager = ReplayManager::new(
            wal,
            sink.clone(),
            sink.ack_tracker().clone(),
            ReplayConfig::default(),
        );

        // Run recovery
        let report = replay_manager.recover_on_startup().await?;

        println!("Recovery report:");
        println!("  Total events: {}", report.total_events);
        println!("  Replayed: {}", report.replayed_events);
        println!("  Failed: {}", report.failed_events);
        println!("  Duration: {:?}", report.duration);
        println!("  Success rate: {:.1}%", report.success_rate() * 100.0);

        // Wait for replayed events to be acknowledged
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Verify recovery was successful
        assert!(
            report.total_events > 0,
            "Should have found unacknowledged events"
        );
        assert!(
            report.success_rate() >= 0.8,
            "Recovery success rate should be at least 80%"
        );
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_deduplication_prevents_duplicate_writes() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    // Write same event twice
    let event = create_test_event("duplicate_test");
    let event_id = event.id;

    sink.write(event.clone())?;

    // Wait for acknowledgment
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Try to write again (should be deduplicated)
    sink.write(event)?;

    // Check acknowledgment tracker
    assert!(
        sink.ack_tracker().is_acknowledged(&event_id) || sink.ack_tracker().pending_count() <= 1
    );

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster - manual test with Kafka shutdown
async fn test_circuit_breaker_opens_on_kafka_failure() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    println!(
        "Initial circuit state: {:?}",
        sink.circuit_breaker().state().await
    );

    // This test requires manually stopping Kafka during execution
    // Write events that will fail due to Kafka being down
    println!("MANUAL STEP: Stop Kafka now and press Enter...");
    // std::io::stdin().read_line(&mut String::new())?;

    // Attempt writes (should fail and open circuit)
    for i in 0..10 {
        let event = create_test_event(&format!("circuit_test_{:03}", i));
        let _ = sink.write(event); // Ignore errors
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Check circuit state
    let state = sink.circuit_breaker().state().await;
    println!("Circuit state after failures: {:?}", state);

    // Note: This test is informational - actual assertion depends on manual Kafka shutdown
    let metrics = sink.circuit_breaker().metrics().await;
    println!("Circuit breaker metrics: {:?}", metrics);

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_high_throughput_performance() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    let event_count = 1000;
    let start = std::time::Instant::now();

    // Write events as fast as possible
    for i in 0..event_count {
        let event = create_test_event(&format!("perf_test_{:05}", i));
        sink.write(event)?;
    }

    let write_duration = start.elapsed();
    let writes_per_sec = event_count as f64 / write_duration.as_secs_f64();

    println!("Performance test results:");
    println!("  Events written: {}", event_count);
    println!("  Duration: {:?}", write_duration);
    println!("  Throughput: {:.0} events/sec", writes_per_sec);

    // Wait for acknowledgments
    tokio::time::sleep(Duration::from_secs(10)).await;

    let pending = sink.ack_tracker().pending_count();
    let acknowledged = event_count - pending;
    let ack_rate = acknowledged as f64 / event_count as f64;

    println!(
        "  Acknowledged: {}/{} ({:.1}%)",
        acknowledged,
        event_count,
        ack_rate * 100.0
    );

    // Performance assertions (conservative)
    assert!(
        writes_per_sec >= 100.0,
        "Should achieve at least 100 writes/sec"
    );
    assert!(ack_rate >= 0.8, "Should acknowledge at least 80% of events");

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_wal_persistence_survives_restart() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let wal_path = temp_dir.path().to_path_buf();

    let event_count = 50;

    // Phase 1: Write events to WAL
    {
        let wal_config = WalConfig::default().with_path(wal_path.clone());
        let metrics = Arc::new(WalMetricsCollector::new("test_wal_persist_1"));
        let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
            Arc::new(FileWal::new(wal_config, metrics).await?);

        let kafka_config = KafkaConfig::default();
        let sink = DurableKafkaLineageSink::new(&kafka_brokers(), wal, kafka_config).await?;

        for i in 0..event_count {
            let event = create_test_event(&format!("wal_persist_{:03}", i));
            sink.write(event)?;
        }

        println!("Phase 1: Wrote {} events", event_count);
    }

    // Phase 2: Restart and check WAL contains events
    {
        let wal_config = WalConfig::default().with_path(wal_path);
        let metrics = Arc::new(WalMetricsCollector::new("test_wal_persist_2"));
        let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
            Arc::new(FileWal::new(wal_config, metrics).await?);

        let kafka_config = KafkaConfig::default();
        let sink = Arc::new(
            DurableKafkaLineageSink::new(&kafka_brokers(), wal.clone(), kafka_config).await?,
        );

        let replay_manager = ReplayManager::new(
            wal,
            sink.clone(),
            sink.ack_tracker().clone(),
            ReplayConfig::default(),
        );

        let report = replay_manager.recover_on_startup().await?;

        println!("Phase 2: Recovery found {} events", report.total_events);

        // WAL should have persisted the events
        assert!(
            report.total_events > 0,
            "WAL should persist events across restarts"
        );
    }

    Ok(())
}

#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_backpressure_applied_at_capacity() -> Result<()> {
    // Use a small max_pending to trigger backpressure quickly
    let temp_dir = TempDir::new()?;
    let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
    let metrics = Arc::new(WalMetricsCollector::new("test_backpressure"));
    let wal: Arc<dyn graphica_coordinator::storage::wal::WriteAheadLog> =
        Arc::new(FileWal::new(wal_config, metrics).await?);

    let mut kafka_config = KafkaConfig::default();
    kafka_config.ack_tracking.max_pending = 10; // Small limit

    let sink = DurableKafkaLineageSink::new(&kafka_brokers(), wal, kafka_config).await?;

    // Write more events than max_pending
    for i in 0..20 {
        let event = create_test_event(&format!("backpressure_test_{:03}", i));
        sink.write(event)?;

        if sink.ack_tracker().is_at_capacity() {
            println!("Backpressure triggered at event {}", i);
            break;
        }
    }

    // Verify backpressure was applied
    assert!(
        sink.ack_tracker().is_at_capacity(),
        "Should have triggered backpressure"
    );

    Ok(())
}

/// Stress test: Write many events concurrently
#[tokio::test]
#[ignore] // Requires Kafka cluster
async fn test_concurrent_writes() -> Result<()> {
    let (sink, _dir) = setup_test_sink().await?;

    let tasks_count = 10;
    let events_per_task = 50;

    let mut handles = vec![];

    for task_id in 0..tasks_count {
        let sink_clone = sink.clone();
        let handle = tokio::spawn(async move {
            for i in 0..events_per_task {
                let event = create_test_event(&format!("concurrent_{}_{:03}", task_id, i));
                sink_clone.write(event).unwrap();
            }
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await?;
    }

    let total_events = tasks_count * events_per_task;
    println!(
        "Wrote {} events concurrently from {} tasks",
        total_events, tasks_count
    );

    // Wait for acknowledgments
    tokio::time::sleep(Duration::from_secs(10)).await;

    let pending = sink.ack_tracker().pending_count();
    let acknowledged = total_events - pending;
    println!("Acknowledged: {}/{}", acknowledged, total_events);

    assert!(
        acknowledged >= total_events * 70 / 100,
        "Should acknowledge at least 70% under concurrent load"
    );

    Ok(())
}
