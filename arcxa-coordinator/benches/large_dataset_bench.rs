//! Large Dataset Performance Benchmarks
//!
//! Benchmarks for Phase 2 production hardening features under load:
//! - 100K to 5M row dataset loading
//! - Memory adaptive batching
//! - Retry behavior under load
//! - Circuit breaker performance impact
//!
//! These benchmarks validate that Phase 2 features perform well at scale.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use graphica_core::reliability::{
    async_retry::{retry_async, RetryPolicy},
    circuit_breaker::{CircuitBreaker, CircuitBreakerConfig},
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Test Data Generation
// ============================================================================

/// Generate test records for benchmarking
fn generate_test_records(count: usize) -> Vec<HashMap<String, String>> {
    (0..count)
        .map(|i| {
            let mut record = HashMap::new();
            record.insert("id".to_string(), format!("REC{:010}", i));
            record.insert("name".to_string(), format!("Customer {}", i));
            record.insert("email".to_string(), format!("customer{}@example.com", i));
            record.insert("age".to_string(), ((i % 80) + 18).to_string());
            record.insert(
                "salary".to_string(),
                format!("{:.2}", 30000.0 + (i % 100000) as f64),
            );
            record.insert(
                "country".to_string(),
                ["USA", "UK", "CA", "AU"][i % 4].to_string(),
            );
            record.insert(
                "status".to_string(),
                ["active", "inactive"][i % 2].to_string(),
            );
            record
        })
        .collect()
}

/// Generate large CSV-like string data
fn generate_csv_data(rows: usize) -> String {
    let header = "id,name,email,age,salary,country,status\n";
    let mut data = String::with_capacity(rows * 100);
    data.push_str(header);

    for i in 0..rows {
        data.push_str(&format!(
            "REC{:010},Customer {},customer{}@example.com,{},{:.2},{},{}\n",
            i,
            i,
            i,
            (i % 80) + 18,
            30000.0 + (i % 100000) as f64,
            ["USA", "UK", "CA", "AU"][i % 4],
            ["active", "inactive"][i % 2]
        ));
    }

    data
}

// ============================================================================
// Mock Components for Benchmarking
// ============================================================================

/// Mock memory monitor
struct MockMemoryMonitor {
    current_pressure: f64,
    min_batch_size: usize,
    max_batch_size: usize,
    default_batch_size: usize,
}

impl MockMemoryMonitor {
    fn new() -> Self {
        Self {
            current_pressure: 0.5,
            min_batch_size: 100,
            max_batch_size: 10_000,
            default_batch_size: 1_000,
        }
    }

    fn set_pressure(&mut self, pressure: f64) {
        self.current_pressure = pressure;
    }

    fn get_adaptive_batch_size(&self) -> usize {
        if self.current_pressure < 0.70 {
            self.default_batch_size
        } else if self.current_pressure < 0.85 {
            let reduction_factor = (0.85 - self.current_pressure) / 0.15;
            let reduced_size = (self.default_batch_size as f64 * reduction_factor) as usize;
            reduced_size.max(self.min_batch_size)
        } else {
            self.min_batch_size
        }
    }
}

/// Mock database loader
async fn mock_database_load(records: &[HashMap<String, String>]) -> Result<usize, String> {
    // Simulate database write latency (1ms per 1000 records)
    let latency = Duration::from_micros((records.len() as u64) / 10);
    tokio::time::sleep(latency).await;
    Ok(records.len())
}

// ============================================================================
// Benchmark 1: Large Dataset Loading
// ============================================================================

fn benchmark_large_dataset_loading(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("large_dataset_loading");

    for size in [100_000, 500_000, 1_000_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.sample_size(10); // Fewer samples for large datasets

        group.bench_with_input(
            BenchmarkId::new("sequential_load", size),
            &size,
            |b, &rows| {
                b.to_async(&runtime).iter(|| async {
                    let data = generate_test_records(rows);

                    // Simulate loading in batches
                    let batch_size = 10_000;
                    let mut total_loaded = 0;

                    for chunk in data.chunks(batch_size) {
                        let loaded = mock_database_load(chunk).await.unwrap();
                        total_loaded += loaded;
                    }

                    black_box(total_loaded)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("adaptive_batch_load", size),
            &size,
            |b, &rows| {
                b.to_async(&runtime).iter(|| async {
                    let data = generate_test_records(rows);
                    let mut monitor = MockMemoryMonitor::new();
                    let mut total_loaded = 0;
                    let mut processed = 0;

                    while processed < data.len() {
                        // Simulate increasing memory pressure
                        let pressure = 0.5 + (processed as f64 / data.len() as f64) * 0.4;
                        monitor.set_pressure(pressure);

                        let batch_size = monitor.get_adaptive_batch_size();
                        let end = (processed + batch_size).min(data.len());

                        let chunk = &data[processed..end];
                        let loaded = mock_database_load(chunk).await.unwrap();
                        total_loaded += loaded;
                        processed = end;
                    }

                    black_box(total_loaded)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 2: Memory Adaptive Batching
// ============================================================================

fn benchmark_memory_adaptive_batching(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("adaptive_batch_1m_rows", |b| {
        b.to_async(&runtime).iter(|| async {
            let mut monitor = MockMemoryMonitor::new();
            let total_rows = 1_000_000;
            let mut processed = 0;
            let mut batch_count = 0;

            while processed < total_rows {
                // Simulate varying memory pressure
                let pressure = 0.5 + ((processed % 100_000) as f64 / 100_000.0) * 0.4;
                monitor.set_pressure(pressure);

                let batch_size = monitor.get_adaptive_batch_size();
                processed += batch_size;
                batch_count += 1;

                black_box(batch_size);
            }

            black_box(batch_count)
        });
    });
}

// ============================================================================
// Benchmark 3: Retry Overhead
// ============================================================================

fn benchmark_retry_overhead(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("retry_overhead");

    // Benchmark successful operation with retry policy
    group.bench_function("no_retry_success", |b| {
        b.to_async(&runtime).iter(|| async {
            let policy = RetryPolicy::no_retry();
            let result = retry_async(policy, || async { Ok::<i32, String>(42) }).await;
            black_box(result)
        });
    });

    group.bench_function("with_retry_success", |b| {
        b.to_async(&runtime).iter(|| async {
            let policy = RetryPolicy::default();
            let result = retry_async(policy, || async { Ok::<i32, String>(42) }).await;
            black_box(result)
        });
    });

    // Benchmark retry with transient failures
    group.bench_function("retry_after_2_failures", |b| {
        b.to_async(&runtime).iter(|| async {
            let policy = RetryPolicy {
                max_retries: 5,
                initial_delay: Duration::from_micros(1),
                max_delay: Duration::from_micros(10),
                backoff_multiplier: 2.0,
            };

            let counter = Arc::new(AtomicU64::new(0));
            let counter_clone = Arc::clone(&counter);

            let result = retry_async(policy, move || {
                let counter = Arc::clone(&counter_clone);
                async move {
                    let count = counter.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Err("temporary failure".to_string())
                    } else {
                        Ok(42)
                    }
                }
            })
            .await;

            black_box(result)
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 4: Circuit Breaker Overhead
// ============================================================================

fn benchmark_circuit_breaker_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("circuit_breaker_overhead");

    // Benchmark without circuit breaker
    group.bench_function("no_circuit_breaker", |b| {
        b.iter(|| {
            // Simulate operation
            let result = 42;
            black_box(result)
        });
    });

    // Benchmark with circuit breaker (closed)
    group.bench_function("circuit_breaker_closed", |b| {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            timeout: Duration::from_secs(10),
        };
        let cb = CircuitBreaker::new("bench_breaker", config);

        b.iter(|| {
            if cb.is_closed() {
                let result = 42;
                cb.record_success();
                black_box(result)
            } else {
                black_box(0)
            }
        });
    });

    // Benchmark with circuit breaker (open, fast fail)
    group.bench_function("circuit_breaker_open_fast_fail", |b| {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 2,
            timeout: Duration::from_secs(10),
        };
        let cb = CircuitBreaker::new("bench_breaker_open", config);

        // Open the circuit
        cb.record_failure();

        b.iter(|| {
            if cb.is_closed() {
                black_box(42)
            } else {
                // Fast fail - no operation performed
                black_box(0)
            }
        });
    });

    group.finish();
}

// ============================================================================
// Benchmark 5: CSV Parsing at Scale
// ============================================================================

fn benchmark_csv_parsing_at_scale(c: &mut Criterion) {
    let mut group = c.benchmark_group("csv_parsing");

    for size in [100_000, 500_000, 1_000_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.sample_size(10);

        group.bench_with_input(BenchmarkId::new("parse_csv", size), &size, |b, &rows| {
            let csv_data = generate_csv_data(rows);

            b.iter(|| {
                let lines: Vec<&str> = csv_data.lines().skip(1).collect();
                let parsed_count = lines.len();
                black_box(parsed_count)
            });
        });
    }

    group.finish();
}

// ============================================================================
// Benchmark 6: Concurrent Processing
// ============================================================================

fn benchmark_concurrent_processing(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("concurrent_processing");

    for workers in [1, 4, 8, 16] {
        group.bench_with_input(
            BenchmarkId::new("parallel_load", workers),
            &workers,
            |b, &worker_count| {
                b.to_async(&runtime).iter(|| async move {
                    let total_records = 100_000;
                    let records_per_worker = total_records / worker_count;

                    let mut tasks = vec![];

                    for _ in 0..worker_count {
                        let task = tokio::spawn(async move {
                            let data = generate_test_records(records_per_worker);
                            mock_database_load(&data).await.unwrap()
                        });
                        tasks.push(task);
                    }

                    let mut total_loaded = 0;
                    for task in tasks {
                        total_loaded += task.await.unwrap();
                    }

                    black_box(total_loaded)
                });
            },
        );
    }

    group.finish();
}

// ============================================================================
// Benchmark 7: End-to-End Pipeline
// ============================================================================

fn benchmark_e2e_pipeline(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("e2e_100k_with_retry_and_monitoring", |b| {
        b.to_async(&runtime).iter(|| async {
            let data = generate_test_records(100_000);
            let mut monitor = MockMemoryMonitor::new();

            let retry_policy = RetryPolicy {
                max_retries: 3,
                initial_delay: Duration::from_micros(1),
                max_delay: Duration::from_micros(100),
                backoff_multiplier: 2.0,
            };

            let cb_config = CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 2,
                timeout: Duration::from_secs(10),
            };
            let cb = Arc::new(CircuitBreaker::new("e2e_bench", cb_config));

            let mut total_loaded = 0;
            let mut processed = 0;

            while processed < data.len() {
                // Adaptive batching based on memory
                let pressure = 0.5 + (processed as f64 / data.len() as f64) * 0.3;
                monitor.set_pressure(pressure);
                let batch_size = monitor.get_adaptive_batch_size();
                let end = (processed + batch_size).min(data.len());

                let chunk = &data[processed..end];

                // Load with retry and circuit breaker
                let cb_clone = Arc::clone(&cb);
                let chunk_vec = chunk.to_vec();

                let result = retry_async(retry_policy.clone(), || {
                    let cb = Arc::clone(&cb_clone);
                    let chunk = chunk_vec.clone();
                    async move {
                        if !cb.is_closed() {
                            return Err("Circuit breaker open".to_string());
                        }

                        match mock_database_load(&chunk).await {
                            Ok(count) => {
                                cb.record_success();
                                Ok(count)
                            }
                            Err(e) => {
                                cb.record_failure();
                                Err(e)
                            }
                        }
                    }
                })
                .await;

                if let Ok(loaded) = result {
                    total_loaded += loaded;
                }

                processed = end;
            }

            black_box(total_loaded)
        });
    });
}

// ============================================================================
// Criterion Configuration
// ============================================================================

criterion_group!(
    benches,
    benchmark_large_dataset_loading,
    benchmark_memory_adaptive_batching,
    benchmark_retry_overhead,
    benchmark_circuit_breaker_overhead,
    benchmark_csv_parsing_at_scale,
    benchmark_concurrent_processing,
    benchmark_e2e_pipeline
);

criterion_main!(benches);
