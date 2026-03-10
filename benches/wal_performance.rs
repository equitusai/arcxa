//! WAL Performance Benchmarks
//!
//! Validates P0 Issue #4: Real performance measurements for WAL overhead
//!
//! This benchmark suite measures:
//! 1. Write latency with/without WAL (p50, p95, p99)
//! 2. Single-threaded throughput
//! 3. Multi-threaded throughput
//! 4. WAL overhead quantification
//! 5. Bottleneck identification

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use chrono::Utc;
use graphica::governance::rdf_star::AnnotatedTriple;
use graphica::governance::rdf_store::GraphicaRdfStore;
use graphica::governance::TransactionId;
use hdrhistogram::Histogram;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// Import WAL types at top level
use graphica::governance::bitemporal::wal::{WriteAheadLog as BitemporalWAL, WalOperation};

// ========================================
// Benchmark 1: Write Latency WITHOUT WAL
// ========================================

fn bench_write_latency_no_wal(c: &mut Criterion) {
    let mut group = c.benchmark_group("write_latency_no_wal");

    group.bench_function("single_insert_no_wal", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
            .unwrap();

        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;

            let triple = AnnotatedTriple::new(
                "http://example.org/entity/perf_test",
                "http://example.org/prop/value",
                &format!("value_{}", counter),
            );

            store.insert_rdf_star_triple(black_box(&triple), None).unwrap();
        });
    });

    group.finish();
}

// ========================================
// Benchmark 2: Write Latency WITH WAL
// ========================================

fn bench_write_latency_with_wal(c: &mut Criterion) {
    let mut group = c.benchmark_group("write_latency_with_wal");

    group.bench_function("single_insert_with_wal", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
            .unwrap()
            .with_temporal_indexes(temp_dir.path().join("indexes"))
            .unwrap();

        let mut counter = 0u64;

        b.iter(|| {
            let tx = TransactionId::new(counter, Utc::now(), 1);
            counter += 1;

            let triple = AnnotatedTriple::new(
                "http://example.org/entity/perf_test",
                "http://example.org/prop/value",
                &format!("value_{}", counter),
            )
            .with_valid_time(Utc::now(), None)
            .with_transaction_time(&tx, None);

            store.insert_rdf_star_triple(black_box(&triple), None).unwrap();
        });
    });

    group.finish();
}

// ========================================
// Benchmark 3: Percentile Latency Analysis
// ========================================

fn bench_latency_percentiles(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_percentiles");

    // WITHOUT WAL
    group.bench_function("percentiles_no_wal", |b| {
        b.iter_custom(|iters| {
            let temp_dir = TempDir::new().unwrap();
            let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                .unwrap();

            let mut hist = Histogram::<u64>::new(3).unwrap();

            for i in 0..iters {
                let triple = AnnotatedTriple::new(
                    "http://example.org/entity/perf_test",
                    "http://example.org/prop/value",
                    &format!("value_{}", i),
                );

                let start = Instant::now();
                store.insert_rdf_star_triple(&triple, None).unwrap();
                let duration = start.elapsed();

                hist.record(duration.as_micros() as u64).unwrap();
            }

            // Report percentiles
            println!("\n=== WITHOUT WAL Latency Percentiles ===");
            println!("p50: {} µs", hist.value_at_percentile(50.0));
            println!("p95: {} µs", hist.value_at_percentile(95.0));
            println!("p99: {} µs", hist.value_at_percentile(99.0));
            println!("max: {} µs", hist.max());

            Duration::from_micros(hist.mean() as u64)
        });
    });

    // WITH WAL
    group.bench_function("percentiles_with_wal", |b| {
        b.iter_custom(|iters| {
            let temp_dir = TempDir::new().unwrap();
            let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                .unwrap()
                .with_temporal_indexes(temp_dir.path().join("indexes"))
                .unwrap();

            let mut hist = Histogram::<u64>::new(3).unwrap();

            for i in 0..iters {
                let tx = TransactionId::new(i, Utc::now(), 1);
                let triple = AnnotatedTriple::new(
                    "http://example.org/entity/perf_test",
                    "http://example.org/prop/value",
                    &format!("value_{}", i),
                )
                .with_valid_time(Utc::now(), None)
                .with_transaction_time(&tx, None);

                let start = Instant::now();
                store.insert_rdf_star_triple(&triple, None).unwrap();
                let duration = start.elapsed();

                hist.record(duration.as_micros() as u64).unwrap();
            }

            // Report percentiles
            println!("\n=== WITH WAL Latency Percentiles ===");
            println!("p50: {} µs", hist.value_at_percentile(50.0));
            println!("p95: {} µs", hist.value_at_percentile(95.0));
            println!("p99: {} µs", hist.value_at_percentile(99.0));
            println!("max: {} µs", hist.max());

            Duration::from_micros(hist.mean() as u64)
        });
    });

    group.finish();
}

// ========================================
// Benchmark 4: Single-Threaded Throughput
// ========================================

fn bench_single_threaded_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_threaded_throughput");

    for batch_size in [100, 1_000, 10_000].iter() {
        // WITHOUT WAL
        group.bench_with_input(
            BenchmarkId::new("no_wal", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter_custom(|_iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                        .unwrap();

                    let start = Instant::now();

                    for i in 0..batch_size {
                        let triple = AnnotatedTriple::new(
                            &format!("http://example.org/entity/{}", i),
                            "http://example.org/prop/value",
                            &format!("value_{}", i),
                        );

                        store.insert_rdf_star_triple(&triple, None).unwrap();
                    }

                    let duration = start.elapsed();
                    let ops_per_sec = (batch_size as f64 / duration.as_secs_f64()) as u64;

                    println!("\nNO WAL - {} inserts: {} ops/sec",
                        batch_size, ops_per_sec);

                    duration
                });
            },
        );

        // WITH WAL
        group.bench_with_input(
            BenchmarkId::new("with_wal", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter_custom(|_iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                        .unwrap()
                        .with_temporal_indexes(temp_dir.path().join("indexes"))
                        .unwrap();

                    let start = Instant::now();

                    for i in 0..batch_size {
                        let tx = TransactionId::new(i, Utc::now(), 1);
                        let triple = AnnotatedTriple::new(
                            &format!("http://example.org/entity/{}", i),
                            "http://example.org/prop/value",
                            &format!("value_{}", i),
                        )
                        .with_valid_time(Utc::now(), None)
                        .with_transaction_time(&tx, None);

                        store.insert_rdf_star_triple(&triple, None).unwrap();
                    }

                    let duration = start.elapsed();
                    let ops_per_sec = (batch_size as f64 / duration.as_secs_f64()) as u64;

                    println!("\nWITH WAL - {} inserts: {} ops/sec",
                        batch_size, ops_per_sec);

                    duration
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 5: Multi-Threaded Throughput
// ========================================

fn bench_multi_threaded_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_threaded_throughput");

    for num_threads in [2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("with_wal", num_threads),
            num_threads,
            |b, &num_threads| {
                b.iter_custom(|_iters| {
                    let temp_dir = TempDir::new().unwrap();
                    let store = Arc::new(
                        GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                            .unwrap()
                            .with_temporal_indexes(temp_dir.path().join("indexes"))
                            .unwrap()
                    );

                    let ops_per_thread = 1_000;
                    let start = Instant::now();

                    let mut handles = Vec::new();

                    for thread_id in 0..num_threads {
                        let store_clone = Arc::clone(&store);

                        let handle = std::thread::spawn(move || {
                            for i in 0..ops_per_thread {
                                let tx = TransactionId::new(
                                    (thread_id * ops_per_thread + i) as u64,
                                    Utc::now(),
                                    1,
                                );
                                let triple = AnnotatedTriple::new(
                                    &format!("http://example.org/entity/t{}/{}", thread_id, i),
                                    "http://example.org/prop/value",
                                    &format!("value_{}", i),
                                )
                                .with_valid_time(Utc::now(), None)
                                .with_transaction_time(&tx, None);

                                store_clone.insert_rdf_star_triple(&triple, None).unwrap();
                            }
                        });

                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }

                    let duration = start.elapsed();
                    let total_ops = num_threads * ops_per_thread;
                    let ops_per_sec = (total_ops as f64 / duration.as_secs_f64()) as u64;

                    println!("\n{} threads - {} total ops: {} ops/sec",
                        num_threads, total_ops, ops_per_sec);

                    duration
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 6: WAL Overhead Breakdown
// ========================================

fn bench_wal_overhead_breakdown(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_overhead_breakdown");

    group.bench_function("baseline_rdf_insert", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
            .unwrap();

        let mut counter = 0u64;

        b.iter(|| {
            counter += 1;
            let triple = AnnotatedTriple::new(
                "http://example.org/entity/test",
                "http://example.org/prop/value",
                &format!("value_{}", counter),
            );

            store.insert_rdf_star_triple(black_box(&triple), None).unwrap();
        });
    });

    group.bench_function("rdf_insert_plus_indexes", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
            .unwrap()
            .with_temporal_indexes(temp_dir.path().join("indexes"))
            .unwrap();

        let mut counter = 0u64;

        b.iter(|| {
            let tx = TransactionId::new(counter, Utc::now(), 1);
            counter += 1;

            let triple = AnnotatedTriple::new(
                "http://example.org/entity/test",
                "http://example.org/prop/value",
                &format!("value_{}", counter),
            )
            .with_valid_time(Utc::now(), None)
            .with_transaction_time(&tx, None);

            store.insert_rdf_star_triple(black_box(&triple), None).unwrap();
        });
    });

    group.finish();
}

// ========================================
// Benchmark 7: WAL Recovery Performance
// ========================================

fn bench_wal_recovery_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("wal_recovery_performance");

    for num_uncommitted in [10, 100, 1_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_uncommitted),
            num_uncommitted,
            |b, &num_uncommitted| {
                b.iter_batched(
                    || {
                        // Setup: Create uncommitted WAL entries
                        let temp_dir = TempDir::new().unwrap();
                        let wal_dir = temp_dir.path().join("indexes");

                        {
                            let store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                                .unwrap()
                                .with_temporal_indexes(&wal_dir)
                                .unwrap();

                            // Manually create uncommitted WAL entries
                            use graphica::governance::bitemporal::{WriteAheadLog, WalOperation};
                            let wal = WriteAheadLog::new(&wal_dir).unwrap();

                            for i in 0..num_uncommitted {
                                let triple = AnnotatedTriple::new(
                                    &format!("http://example.org/entity/{}", i),
                                    "http://example.org/prop/value",
                                    &format!("value_{}", i),
                                );

                                let operation = WalOperation::InsertTriple {
                                    triple_json: serde_json::to_string(&triple).unwrap(),
                                    version_id: format!("v{}", i),
                                    tx_id_json: "null".to_string(),
                                    graph_uri: None,
                                };

                                wal.log_operation(operation).unwrap();
                            }
                        }

                        (temp_dir, wal_dir)
                    },
                    |(temp_dir, wal_dir)| {
                        // Benchmark: Recovery time
                        let start = Instant::now();

                        let _store = GraphicaRdfStore::new(temp_dir.path().join("rdf_store"))
                            .unwrap()
                            .with_temporal_indexes(&wal_dir)
                            .unwrap();

                        let recovery_duration = start.elapsed();

                        println!("\nRecovery of {} uncommitted ops: {:?}",
                            num_uncommitted, recovery_duration);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_write_latency_no_wal,
    bench_write_latency_with_wal,
    bench_latency_percentiles,
    bench_single_threaded_throughput,
    bench_multi_threaded_throughput,
    bench_wal_overhead_breakdown,
    bench_wal_recovery_performance,
);

criterion_main!(benches);
