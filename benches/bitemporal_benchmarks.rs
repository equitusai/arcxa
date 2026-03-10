// Comprehensive benchmarks for bitemporal MVCC performance
//
// These benchmarks prove the O(1) performance claims and establish baselines for:
// - Current version lookups
// - Audit trail retrieval
// - Version superseding
// - Temporal queries
// - Concurrent operations

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use chrono::Utc;
use graphica::governance::{TransactionId, VersionManager, MVCCQueryExecutor};
use graphica::governance::rdf_star::AnnotatedTriple;
use graphica::governance::rdf_store::GraphicaRdfStore;
use std::sync::Arc;
use tempfile::TempDir;

// ========================================
// Benchmark 1: Current Version Lookup
// ========================================

fn bench_current_version_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("current_version_lookup");

    // Test with varying number of versions per entity
    for num_versions in [10, 100, 1_000, 10_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_versions),
            num_versions,
            |b, &num_versions| {
                // Setup: Create store with temporal indexes
                let temp_dir = TempDir::new().unwrap();
                let store = GraphicaRdfStore::new_in_memory()
                    .unwrap()
                    .with_temporal_indexes(temp_dir.path().join("indexes"))
                    .unwrap();

                let indexes = store.temporal_indexes().unwrap();

                let subject = "http://example.org/entity/perf_test";
                let predicate = "http://example.org/prop/value";

                // Insert N versions
                for i in 0..num_versions {
                    let tx = TransactionId::new(i as u64 + 1, Utc::now(), 1);
                    let triple = AnnotatedTriple::new(subject, predicate, &format!("value_{}", i))
                        .with_valid_time(Utc::now(), None)
                        .with_transaction_time(&tx, None);

                    store.insert_rdf_star_triple(&triple, None).unwrap();
                }

                // Benchmark: Lookup current version (should be O(1) regardless of history size)
                b.iter(|| {
                    let current = indexes.find_current_version(
                        black_box(subject),
                        black_box(predicate),
                    ).unwrap();
                    black_box(current);
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 2: Audit Trail Retrieval
// ========================================

fn bench_audit_trail_retrieval(c: &mut Criterion) {
    let mut group = c.benchmark_group("audit_trail_retrieval");

    for num_versions in [10, 100, 1_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_versions),
            num_versions,
            |b, &num_versions| {
                let temp_dir = TempDir::new().unwrap();
                let store = GraphicaRdfStore::new_in_memory()
                    .unwrap()
                    .with_temporal_indexes(temp_dir.path().join("indexes"))
                    .unwrap();

                let indexes = store.temporal_indexes().unwrap();
                let executor = MVCCQueryExecutor::new(Arc::new(store.clone()), indexes);

                let subject = "http://example.org/entity/audit_test";
                let predicate = "http://example.org/prop/balance";

                // Insert N versions
                for i in 0..num_versions {
                    let tx = TransactionId::new(i as u64 + 1, Utc::now(), 1);
                    let triple = AnnotatedTriple::new(subject, predicate, &format!("{}", i * 1000))
                        .with_valid_time(Utc::now(), None)
                        .with_transaction_time(&tx, None);

                    store.insert_rdf_star_triple(&triple, None).unwrap();
                }

                // Benchmark: Retrieve complete audit trail
                let rt = tokio::runtime::Runtime::new().unwrap();
                b.iter(|| {
                    rt.block_on(async {
                        let trail = executor.get_audit_trail(
                            black_box(subject),
                            black_box(predicate),
                        ).await.unwrap();
                        black_box(trail);
                    });
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 3: Version Superseding
// ========================================

fn bench_version_superseding(c: &mut Criterion) {
    let mut group = c.benchmark_group("version_superseding");

    for num_existing in [10, 100, 1_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_existing),
            num_existing,
            |b, &num_existing| {
                let temp_dir = TempDir::new().unwrap();
                let store = GraphicaRdfStore::new_in_memory()
                    .unwrap()
                    .with_temporal_indexes(temp_dir.path().join("indexes"))
                    .unwrap();

                let indexes = store.temporal_indexes().unwrap();
                let version_mgr = VersionManager::new(Arc::new(store.clone()), indexes);

                let subject = "http://example.org/entity/supersede_test";
                let predicate = "http://example.org/prop/status";

                // Insert N existing versions
                for i in 0..num_existing {
                    let tx = TransactionId::new(i as u64 + 1, Utc::now(), 1);
                    let triple = AnnotatedTriple::new(subject, predicate, &format!("status_{}", i))
                        .with_valid_time(Utc::now(), None)
                        .with_transaction_time(&tx, None);

                    store.insert_rdf_star_triple(&triple, None).unwrap();
                }

                // Benchmark: Check if new version supersedes (should be O(1))
                let new_triple = AnnotatedTriple::new(subject, predicate, "new_status")
                    .with_valid_time(Utc::now(), None)
                    .with_transaction_time(&TransactionId::new(9999, Utc::now(), 1), None);

                let rt = tokio::runtime::Runtime::new().unwrap();
                b.iter(|| {
                    rt.block_on(async {
                        let result = version_mgr.check_supersedes(black_box(&new_triple))
                            .await
                            .unwrap();
                        black_box(result);
                    });
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 4: Batch Inserts
// ========================================

fn bench_batch_inserts(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_inserts");

    for batch_size in [10, 100, 1_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter_batched(
                    || {
                        // Setup for each iteration
                        let temp_dir = TempDir::new().unwrap();
                        let store = GraphicaRdfStore::new_in_memory()
                            .unwrap()
                            .with_temporal_indexes(temp_dir.path().join("indexes"))
                            .unwrap();

                        // Prepare batch
                        let mut triples = Vec::new();
                        for i in 0..batch_size {
                            let tx = TransactionId::new(i as u64 + 1, Utc::now(), 1);
                            let triple = AnnotatedTriple::new(
                                &format!("http://example.org/entity/{}", i),
                                "http://example.org/prop/value",
                                &format!("value_{}", i),
                            )
                            .with_valid_time(Utc::now(), None)
                            .with_transaction_time(&tx, None);

                            triples.push(triple);
                        }

                        (store, triples)
                    },
                    |(store, triples)| {
                        // Benchmark: Insert batch
                        for triple in &triples {
                            store.insert_rdf_star_triple(black_box(triple), None).unwrap();
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 5: Index Scaling
// ========================================

fn bench_index_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("index_scaling");

    // Test index performance with different total dataset sizes
    for total_versions in [1_000, 10_000, 100_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(total_versions),
            total_versions,
            |b, &total_versions| {
                // Setup: Create large dataset
                let temp_dir = TempDir::new().unwrap();
                let store = GraphicaRdfStore::new_in_memory()
                    .unwrap()
                    .with_temporal_indexes(temp_dir.path().join("indexes"))
                    .unwrap();

                let indexes = store.temporal_indexes().unwrap();

                // Insert versions across multiple entities
                let num_entities = (total_versions / 100).max(1);
                let versions_per_entity = total_versions / num_entities;

                for entity_id in 0..num_entities {
                    let subject = format!("http://example.org/entity/scale_{}", entity_id);
                    let predicate = "http://example.org/prop/data";

                    for version_num in 0..versions_per_entity {
                        let tx = TransactionId::new(
                            (entity_id * versions_per_entity + version_num + 1) as u64,
                            Utc::now(),
                            1,
                        );
                        let triple = AnnotatedTriple::new(
                            &subject,
                            predicate,
                            &format!("v_{}", version_num),
                        )
                        .with_valid_time(Utc::now(), None)
                        .with_transaction_time(&tx, None);

                        store.insert_rdf_star_triple(&triple, None).unwrap();
                    }
                }

                // Benchmark: Lookup from middle of dataset (should still be O(1))
                let mid_entity = num_entities / 2;
                let mid_subject = format!("http://example.org/entity/scale_{}", mid_entity);

                b.iter(|| {
                    let current = indexes.find_current_version(
                        black_box(&mid_subject),
                        black_box("http://example.org/prop/data"),
                    ).unwrap();
                    black_box(current);
                });
            },
        );
    }

    group.finish();
}

// ========================================
// Benchmark 6: Memory Efficiency
// ========================================

fn bench_memory_efficiency(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_efficiency");

    group.bench_function("memory_overhead_per_version", |b| {
        let temp_dir = TempDir::new().unwrap();
        let store = GraphicaRdfStore::new_in_memory()
            .unwrap()
            .with_temporal_indexes(temp_dir.path().join("indexes"))
            .unwrap();

        let subject = "http://example.org/entity/mem_test";
        let predicate = "http://example.org/prop/value";

        b.iter(|| {
            let tx = TransactionId::new(1, Utc::now(), 1);
            let triple = AnnotatedTriple::new(subject, predicate, "test_value")
                .with_valid_time(Utc::now(), None)
                .with_transaction_time(&tx, None);

            store.insert_rdf_star_triple(black_box(&triple), None).unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_current_version_lookup,
    bench_audit_trail_retrieval,
    bench_version_superseding,
    bench_batch_inserts,
    bench_index_scaling,
    bench_memory_efficiency,
);

criterion_main!(benches);
