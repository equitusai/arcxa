//! Performance benchmarks for Kafka durability implementation
//!
//! Measures:
//! - WAL write throughput and latency
//! - End-to-end write latency (WAL + Kafka)
//! - Acknowledgment tracker performance
//! - Circuit breaker overhead
//! - Batch write performance
//!
//! # Running Benchmarks
//!
//! ```bash
//! # Run all Kafka benchmarks
//! cargo bench --bench kafka_throughput_bench
//!
//! # Run specific benchmark
//! cargo bench --bench kafka_throughput_bench -- wal_write
//!
//! # Generate HTML report
//! cargo bench --bench kafka_throughput_bench -- --output-format html
//! ```
//!
//! # Requirements
//!
//! - Kafka cluster running (for end-to-end benchmarks)
//! - Set KAFKA_BENCH_BROKERS environment variable (default: localhost:9092)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use graphica_coordinator::storage::kafka::{
    AcknowledgmentTracker, CircuitBreaker, CircuitBreakerConfig, DurableKafkaLineageSink,
    KafkaConfig,
};
use graphica_coordinator::storage::wal::{
    EntryPayload, EntryType, FileWal, LogSequenceNumber, WalConfig, WalEntry, WalMetricsCollector,
    WriteAheadLog,
};
use graphica_core::core::lineage::{DataRef, LineageEvent, LineageSink};
use std::sync::Arc;
use tempfile::TempDir;
use uuid::Uuid;

fn kafka_brokers() -> String {
    std::env::var("KAFKA_BENCH_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string())
}

fn create_test_event(record_id: &str) -> LineageEvent {
    use chrono::Utc;
    use std::collections::HashMap;

    LineageEvent {
        id: Uuid::new_v4(),
        dataset: "bench_dataset".to_string(),
        record_id: record_id.to_string(),
        source_refs: vec![DataRef {
            system: "bench_system".to_string(),
            path: "bench/path".to_string(),
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
        run_id: "bench_run".to_string(),
        tenant_id: "bench_tenant".to_string(),
        correlation_id: None,
        metadata: HashMap::new(),
    }
}

/// Benchmark WAL write performance (hot path)
fn bench_wal_write(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("wal_write");
    group.throughput(Throughput::Elements(1));

    // Setup WAL
    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
    let metrics = Arc::new(WalMetricsCollector::new("bench_wal"));
    let wal = runtime.block_on(async { FileWal::new(wal_config, metrics).await.unwrap() });
    let wal = Arc::new(wal);

    group.bench_function("single_write", |b| {
        b.to_async(&runtime).iter(|| async {
            let event = black_box(create_test_event("bench_001"));
            let entry = WalEntry::new(
                LogSequenceNumber::ZERO,
                EntryType::KafkaPublish,
                EntryPayload::Lineage(Box::new(event)),
            );
            let _lsn = wal.append(entry).await.unwrap();
        });
    });

    group.bench_function("write_and_sync", |b| {
        b.to_async(&runtime).iter(|| async {
            let event = black_box(create_test_event("bench_002"));
            let entry = WalEntry::new(
                LogSequenceNumber::ZERO,
                EntryType::KafkaPublish,
                EntryPayload::Lineage(Box::new(event)),
            );
            let _lsn = wal.append(entry).await.unwrap();
            wal.sync().await.unwrap();
        });
    });

    group.finish();
}

/// Benchmark acknowledgment tracker operations
fn bench_acknowledgment_tracker(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("acknowledgment_tracker");
    group.throughput(Throughput::Elements(1));

    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
    let metrics = Arc::new(WalMetricsCollector::new("bench_ack"));
    let wal = runtime.block_on(async { FileWal::new(wal_config, metrics).await.unwrap() });
    let wal: Arc<dyn WriteAheadLog> = Arc::new(wal);

    let kafka_config = KafkaConfig::default();
    let tracker = AcknowledgmentTracker::new(wal.clone(), kafka_config.ack_tracking);

    group.bench_function("track_pending", |b| {
        b.iter(|| {
            let event_id = black_box(Uuid::new_v4());
            let lsn = black_box(LogSequenceNumber(42));
            let event = black_box(create_test_event("bench_track"));
            tracker.track_pending(event_id, lsn, event);
        });
    });

    group.bench_function("is_acknowledged", |b| {
        let event_id = Uuid::new_v4();
        tracker.track_pending(
            event_id,
            LogSequenceNumber(42),
            create_test_event("bench_check"),
        );

        b.iter(|| {
            let result = tracker.is_acknowledged(&black_box(event_id));
            black_box(result);
        });
    });

    group.bench_function("pending_count", |b| {
        b.iter(|| {
            let count = tracker.pending_count();
            black_box(count);
        });
    });

    group.finish();
}

/// Benchmark circuit breaker overhead
fn bench_circuit_breaker(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("circuit_breaker");
    group.throughput(Throughput::Elements(1));

    let config = CircuitBreakerConfig::default();
    let breaker = CircuitBreaker::new(config);

    group.bench_function("call_noop", |b| {
        b.to_async(&runtime).iter(|| async {
            let _result = breaker.call(|| async { Ok::<(), anyhow::Error>(()) }).await;
        });
    });

    group.bench_function("record_success", |b| {
        b.to_async(&runtime).iter(|| async {
            breaker.record_success().await;
        });
    });

    group.finish();
}

/// Benchmark batch writes (varying batch sizes)
fn bench_batch_writes(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("batch_writes");

    for batch_size in [10, 50, 100, 500].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));

        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
        let metrics = Arc::new(WalMetricsCollector::new("bench_batch"));
        let wal = runtime.block_on(async { FileWal::new(wal_config, metrics).await.unwrap() });
        let wal = Arc::new(wal);

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &size| {
                b.to_async(&runtime).iter(|| async {
                    let events: Vec<LineageEvent> = (0..size)
                        .map(|i| create_test_event(&format!("batch_{}", i)))
                        .collect();

                    for event in events {
                        let entry = WalEntry::new(
                            LogSequenceNumber::ZERO,
                            EntryType::KafkaPublish,
                            EntryPayload::Lineage(Box::new(event)),
                        );
                        let _lsn = wal.append(entry).await.unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark end-to-end write (requires Kafka)
/// This benchmark is marked as optional and requires KAFKA_BENCH_ENABLED=true
#[allow(dead_code)]
fn bench_end_to_end_write(c: &mut Criterion) {
    // Skip if Kafka benchmarking not enabled
    if std::env::var("KAFKA_BENCH_ENABLED").unwrap_or_else(|_| "false".to_string()) != "true" {
        return;
    }

    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("end_to_end");
    group.throughput(Throughput::Elements(1));
    group.sample_size(20); // Reduce sample size for slower benchmarks

    let temp_dir = TempDir::new().unwrap();
    let wal_config = WalConfig::default().with_path(temp_dir.path().to_path_buf());
    let metrics = Arc::new(WalMetricsCollector::new("bench_e2e"));
    let wal = runtime.block_on(async { FileWal::new(wal_config, metrics).await.unwrap() });
    let wal: Arc<dyn WriteAheadLog> = Arc::new(wal);

    let kafka_config = KafkaConfig::default();
    let sink = runtime.block_on(async {
        DurableKafkaLineageSink::new(&kafka_brokers(), wal, kafka_config)
            .await
            .unwrap()
    });

    group.bench_function("durable_write", |b| {
        b.iter(|| {
            let event = black_box(create_test_event("bench_e2e"));
            sink.write(event).unwrap();
        });
    });

    group.finish();
}

/// Benchmark event serialization (lineage event to bytes)
fn bench_event_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");
    group.throughput(Throughput::Elements(1));

    let event = create_test_event("bench_serialize");

    group.bench_function("bincode_serialize", |b| {
        b.iter(|| {
            let bytes = bincode::serialize(&black_box(&event)).unwrap();
            black_box(bytes);
        });
    });

    group.bench_function("bincode_deserialize", |b| {
        let bytes = bincode::serialize(&event).unwrap();
        b.iter(|| {
            let event: LineageEvent = bincode::deserialize(&black_box(&bytes)).unwrap();
            black_box(event);
        });
    });

    group.finish();
}

/// Benchmark UUID generation (used for event IDs)
fn bench_uuid_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("uuid");
    group.throughput(Throughput::Elements(1));

    group.bench_function("uuid_v4", |b| {
        b.iter(|| {
            let id = Uuid::new_v4();
            black_box(id);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_wal_write,
    bench_acknowledgment_tracker,
    bench_circuit_breaker,
    bench_batch_writes,
    bench_event_serialization,
    bench_uuid_generation
);

// Uncomment to include end-to-end benchmarks (requires Kafka)
// criterion_group!(
//     kafka_benches,
//     bench_end_to_end_write
// );

criterion_main!(benches);
