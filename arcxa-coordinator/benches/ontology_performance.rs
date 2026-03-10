//! Performance benchmarks for ontology integration
//!
//! These benchmarks measure the performance improvements from the optimization work:
//! - Priority 1: Term caching
//! - Priority 2: Namespace filtering
//! - Priority 3: RwLock contention fixes
//! - Priority 4: RocksDB persistence
//!
//! Run with: cargo bench --bench ontology_performance

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use graphica_coordinator::mapping::ontology_registry::{PersistedOntologyRegistry, RegistryClient};
use graphica_core::catalog::OntologyRegistry;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Create a test ontology with N terms
fn create_test_ontology(namespace: &str, term_count: usize) -> String {
    let mut ontology = format!("@prefix : <{}> .\n", namespace);
    ontology.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    ontology.push_str("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\n");

    for i in 0..term_count {
        ontology.push_str(&format!(
            ":Property{} a owl:DatatypeProperty ;\n    rdfs:label \"Property {}\" .\n\n",
            i, i
        ));
    }

    ontology
}

/// Benchmark: Parse ontologies with caching (Priority 1)
fn bench_term_caching(c: &mut Criterion) {
    let mut group = c.benchmark_group("term_caching");

    for ontology_count in [5, 10, 20].iter() {
        group.bench_with_input(
            BenchmarkId::new("cold_cache", ontology_count),
            ontology_count,
            |b, &count| {
                b.iter_with_setup(
                    || {
                        // Setup: Create fresh registry with ontologies
                        let mut registry = OntologyRegistry::new();
                        for i in 0..count {
                            let namespace = format!("http://example.com/ont{}#", i);
                            let content = create_test_ontology(&namespace, 50);
                            registry
                                .register_custom_ontology(
                                    &format!("ont{}", i),
                                    content,
                                    Some(namespace),
                                )
                                .unwrap();
                        }

                        // Create client with no cache
                        RegistryClient::new(Some(Arc::new(RwLock::new(registry))))
                    },
                    |client| {
                        // Measure: First call (cache miss)
                        let terms = client.get_ontology_terms().unwrap();
                        black_box(terms);
                    },
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("warm_cache", ontology_count),
            ontology_count,
            |b, &count| {
                // Setup: Create registry and warm up cache
                let mut registry = OntologyRegistry::new();
                for i in 0..count {
                    let namespace = format!("http://example.com/ont{}#", i);
                    let content = create_test_ontology(&namespace, 50);
                    registry
                        .register_custom_ontology(&format!("ont{}", i), content, Some(namespace))
                        .unwrap();
                }

                let client = RegistryClient::new(Some(Arc::new(RwLock::new(registry))));

                // Warm up cache
                let _ = client.get_ontology_terms().unwrap();

                b.iter(|| {
                    // Measure: Subsequent calls (cache hit)
                    let terms = client.get_ontology_terms().unwrap();
                    black_box(terms);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Namespace filtering (Priority 2)
fn bench_namespace_filtering(c: &mut Criterion) {
    let mut group = c.benchmark_group("namespace_filtering");

    // Setup: Create registry with 20 ontologies
    let mut registry = OntologyRegistry::new();
    for i in 0..20 {
        let namespace = format!("http://example.com/ont{}#", i);
        let content = create_test_ontology(&namespace, 50);
        registry
            .register_custom_ontology(&format!("ont{}", i), content, Some(namespace))
            .unwrap();
    }

    let client = RegistryClient::new(Some(Arc::new(RwLock::new(registry))));

    // Benchmark: Filter to 1 namespace
    group.bench_function("filter_1_of_20", |b| {
        b.iter(|| {
            let terms = client
                .get_terms_by_namespaces(&["http://example.com/ont0#".to_string()])
                .unwrap();
            black_box(terms);
        });
    });

    // Benchmark: Filter to 5 namespaces
    group.bench_function("filter_5_of_20", |b| {
        b.iter(|| {
            let terms = client
                .get_terms_by_namespaces(&[
                    "http://example.com/ont0#".to_string(),
                    "http://example.com/ont1#".to_string(),
                    "http://example.com/ont2#".to_string(),
                    "http://example.com/ont3#".to_string(),
                    "http://example.com/ont4#".to_string(),
                ])
                .unwrap();
            black_box(terms);
        });
    });

    // Benchmark: No filter (all ontologies)
    group.bench_function("no_filter", |b| {
        b.iter(|| {
            let terms = client.get_terms_by_namespaces(&[]).unwrap();
            black_box(terms);
        });
    });

    group.finish();
}

/// Benchmark: Concurrent access (Priority 3 - RwLock contention fix)
fn bench_concurrent_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_access");
    group.measurement_time(Duration::from_secs(10));

    // Setup: Create registry with ontologies
    let mut registry = OntologyRegistry::new();
    for i in 0..10 {
        let namespace = format!("http://example.com/ont{}#", i);
        let content = create_test_ontology(&namespace, 50);
        registry
            .register_custom_ontology(&format!("ont{}", i), content, Some(namespace))
            .unwrap();
    }

    let registry = Arc::new(RwLock::new(registry));

    for thread_count in [1, 2, 4, 8].iter() {
        group.bench_with_input(
            BenchmarkId::new("parallel_requests", thread_count),
            thread_count,
            |b, &threads| {
                b.iter(|| {
                    let handles: Vec<_> = (0..threads)
                        .map(|_| {
                            let reg = registry.clone();
                            std::thread::spawn(move || {
                                let client = RegistryClient::new(Some(reg));
                                let terms = client.get_ontology_terms().unwrap();
                                black_box(terms);
                            })
                        })
                        .collect();

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Persistence operations (Priority 4)
fn bench_persistence(c: &mut Criterion) {
    let mut group = c.benchmark_group("persistence");

    // Benchmark: Register and persist ontology
    group.bench_function("register_and_persist", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        b.iter(|| {
            let temp_dir = TempDir::new().unwrap();
            let db_path = temp_dir.path().join("bench.db");

            runtime.block_on(async {
                let registry = PersistedOntologyRegistry::open(&db_path).await.unwrap();
                let content = create_test_ontology("http://bench.com/ont#", 50);

                registry
                    .register_custom_ontology(
                        "bench_ont",
                        content,
                        Some("http://bench.com/ont#".to_string()),
                    )
                    .await
                    .unwrap();

                black_box(registry);
            });
        });
    });

    // Benchmark: Load from disk
    group.bench_function("load_from_disk", |b| {
        let runtime = tokio::runtime::Runtime::new().unwrap();

        b.iter_with_setup(
            || {
                // Setup: Create and persist 10 ontologies
                let temp_dir = TempDir::new().unwrap();
                let db_path = temp_dir.path().join("bench.db");

                runtime.block_on(async {
                    let registry = PersistedOntologyRegistry::open(&db_path).await.unwrap();

                    for i in 0..10 {
                        let namespace = format!("http://example.com/ont{}#", i);
                        let content = create_test_ontology(&namespace, 50);
                        registry
                            .register_custom_ontology(
                                &format!("ont{}", i),
                                content,
                                Some(namespace),
                            )
                            .await
                            .unwrap();
                    }
                });

                (temp_dir, db_path)
            },
            |(temp_dir, db_path)| {
                // Measure: Load all ontologies from disk
                runtime.block_on(async {
                    let registry = PersistedOntologyRegistry::open(&db_path).await.unwrap();
                    black_box(registry);
                });

                drop(temp_dir);
            },
        );
    });

    group.finish();
}

/// Benchmark: Scalability with different ontology sizes
fn bench_scalability(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalability");
    group.measurement_time(Duration::from_secs(15));

    for term_count in [100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::new("parse_terms", term_count),
            term_count,
            |b, &count| {
                let content = create_test_ontology("http://large.com/ont#", count);

                b.iter(|| {
                    let mut registry = OntologyRegistry::new();
                    registry
                        .register_custom_ontology(
                            "large_ont",
                            &content,
                            Some("http://large.com/ont#".to_string()),
                        )
                        .unwrap();

                    let client = RegistryClient::new(Some(Arc::new(RwLock::new(registry))));
                    let terms = client.get_ontology_terms().unwrap();
                    black_box(terms);
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_term_caching,
    bench_namespace_filtering,
    bench_concurrent_access,
    bench_persistence,
    bench_scalability,
);

criterion_main!(benches);
