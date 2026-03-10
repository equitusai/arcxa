//! Performance benchmarks for Unified Ontology Mapping System
//!
//! Run with: cargo bench --bench unified_ontology_bench

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use graphica_coordinator::mapping::field_mapping::{
    FieldDescriptor, FieldStatistics, MappingOptions, UnifiedMappingConfig,
    UnifiedOntologyMappingEngine,
};
use tokio::runtime::Runtime;

// Helper to create test fields with different characteristics
fn create_email_field(id: &str, name: &str) -> FieldDescriptor {
    FieldDescriptor {
        id: id.to_string(),
        name: name.to_string(),
        normalized_name: name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect(),
        data_type: "VARCHAR(255)".to_string(),
        nullable: false,
        primary_key: false,
        sample_values: vec![
            "john.doe@example.com".to_string(),
            "jane.smith@company.org".to_string(),
            "admin@test.net".to_string(),
        ],
        description: None,
        source_id: "benchmark".to_string(),
        table_name: "customers".to_string(),
        statistics: Some(FieldStatistics {
            distinct_count: Some(1000),
            null_count: Some(0),
            total_count: Some(1000),
            min_length: Some(15),
            max_length: Some(50),
            avg_length: Some(25.5),
        }),
    }
}

fn create_phone_field(id: &str, name: &str) -> FieldDescriptor {
    FieldDescriptor {
        id: id.to_string(),
        name: name.to_string(),
        normalized_name: name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect(),
        data_type: "VARCHAR(20)".to_string(),
        nullable: true,
        primary_key: false,
        sample_values: vec![
            "555-1234".to_string(),
            "(123) 456-7890".to_string(),
            "+1-800-555-0199".to_string(),
        ],
        description: None,
        source_id: "benchmark".to_string(),
        table_name: "customers".to_string(),
        statistics: None,
    }
}

fn create_generic_field(id: &str, name: &str, data_type: &str) -> FieldDescriptor {
    FieldDescriptor {
        id: id.to_string(),
        name: name.to_string(),
        normalized_name: name
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect(),
        data_type: data_type.to_string(),
        nullable: true,
        primary_key: false,
        sample_values: vec![
            "value1".to_string(),
            "value2".to_string(),
            "value3".to_string(),
        ],
        description: None,
        source_id: "benchmark".to_string(),
        table_name: "test_table".to_string(),
        statistics: None,
    }
}

// Benchmark single field mapping with cold cache
fn bench_single_field_cold_cache(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    c.bench_function("single_field_email_cold_cache", |b| {
        b.iter(|| {
            rt.block_on(async {
                // Create new engine each iteration (cold cache)
                let config = UnifiedMappingConfig::default();
                let engine = UnifiedOntologyMappingEngine::new(config, None, None)
                    .await
                    .unwrap();
                let field = create_email_field("bench_1", "customer_email");
                let options = MappingOptions::default();

                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });
}

// Benchmark single field mapping with warm cache
fn bench_single_field_warm_cache(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    c.bench_function("single_field_email_warm_cache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_email_field("bench_2", "customer_email");
                let options = MappingOptions::default();

                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });
}

// Benchmark different field types
fn bench_field_types(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("field_types");

    // Email field (strong pattern match)
    group.bench_function("email_field", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_email_field("email_bench", "customer_email");
                let options = MappingOptions::default();
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    // Phone field (pattern match)
    group.bench_function("phone_field", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_phone_field("phone_bench", "contact_phone");
                let options = MappingOptions::default();
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    // Generic field (lexical/heuristic only)
    group.bench_function("generic_field", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_generic_field("generic_bench", "some_column", "VARCHAR(100)");
                let options = MappingOptions::default();
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    group.finish();
}

// Benchmark batch mapping with varying sizes
fn bench_batch_mapping(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("batch_mapping");

    for size in [1, 5, 10, 20, 50].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let fields: Vec<_> = (0..size)
                .map(|i| create_email_field(&format!("batch_{}", i), &format!("email_{}", i)))
                .collect();

            b.iter(|| {
                rt.block_on(async {
                    let options = MappingOptions::default();
                    let result = engine.map_fields(black_box(&fields), &options).await;
                    black_box(result)
                })
            });
        });
    }

    group.finish();
}

// Benchmark with different confidence thresholds
fn bench_confidence_thresholds(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("confidence_thresholds");

    for threshold in [0.5, 0.7, 0.9].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{:.1}", threshold)),
            threshold,
            |b, &threshold| {
                b.iter(|| {
                    rt.block_on(async {
                        let field = create_email_field("threshold_bench", "customer_email");
                        let options = MappingOptions {
                            min_confidence: threshold,
                            ..Default::default()
                        };
                        let result = engine.map_field(black_box(&field), &options).await;
                        black_box(result)
                    })
                });
            },
        );
    }

    group.finish();
}

// Benchmark with different max_candidates values
fn bench_max_candidates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("max_candidates");

    for max in [1, 3, 5, 10].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(max), max, |b, &max| {
            b.iter(|| {
                rt.block_on(async {
                    let field = create_email_field("candidates_bench", "customer_email");
                    let options = MappingOptions {
                        max_candidates: max,
                        ..Default::default()
                    };
                    let result = engine.map_field(black_box(&field), &options).await;
                    black_box(result)
                })
            });
        });
    }

    group.finish();
}

// Benchmark with different numbers of sample values
fn bench_sample_value_counts(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("sample_value_counts");

    for count in [1, 5, 10, 50, 100].iter() {
        group.bench_with_input(BenchmarkId::from_parameter(count), count, |b, &count| {
            let samples: Vec<String> = (0..count)
                .map(|i| format!("user{}@example.com", i))
                .collect();

            let field = FieldDescriptor {
                id: "sample_bench".to_string(),
                name: "email".to_string(),
                normalized_name: "email".to_string(),
                data_type: "VARCHAR(255)".to_string(),
                nullable: false,
                primary_key: false,
                sample_values: samples,
                description: None,
                source_id: "benchmark".to_string(),
                table_name: "test".to_string(),
                statistics: None,
            };

            b.iter(|| {
                rt.block_on(async {
                    let options = MappingOptions::default();
                    let result = engine.map_field(black_box(&field), &options).await;
                    black_box(result)
                })
            });
        });
    }

    group.finish();
}

// Benchmark strategy-specific execution
fn bench_individual_strategies(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    let mut group = c.benchmark_group("individual_strategies");

    // Pattern strategy only
    group.bench_function("pattern_only", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_email_field("pattern_bench", "email");
                let options = MappingOptions {
                    enabled_strategies: Some(vec!["pattern".to_string()]),
                    ..Default::default()
                };
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    // Lexical strategy only
    group.bench_function("lexical_only", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_email_field("lexical_bench", "emails");
                let options = MappingOptions {
                    enabled_strategies: Some(vec!["lexical".to_string()]),
                    ..Default::default()
                };
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    // Heuristic strategy only
    group.bench_function("heuristic_only", |b| {
        b.iter(|| {
            rt.block_on(async {
                let field = create_generic_field("heuristic_bench", "id", "INTEGER");
                let options = MappingOptions {
                    enabled_strategies: Some(vec!["heuristic".to_string()]),
                    ..Default::default()
                };
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    group.finish();
}

// Benchmark realistic workload (mixed field types)
fn bench_realistic_workload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let config = UnifiedMappingConfig::default();
    let engine = rt.block_on(async {
        UnifiedOntologyMappingEngine::new(config, None, None)
            .await
            .unwrap()
    });

    c.bench_function("realistic_table_10_fields", |b| {
        let fields = vec![
            create_email_field("1", "customer_email"),
            create_phone_field("2", "phone_number"),
            create_generic_field("3", "first_name", "VARCHAR(50)"),
            create_generic_field("4", "last_name", "VARCHAR(50)"),
            create_generic_field("5", "address", "TEXT"),
            create_generic_field("6", "city", "VARCHAR(100)"),
            create_generic_field("7", "postal_code", "VARCHAR(20)"),
            create_generic_field("8", "country", "VARCHAR(50)"),
            create_generic_field("9", "created_at", "TIMESTAMP"),
            create_generic_field("10", "updated_at", "TIMESTAMP"),
        ];

        b.iter(|| {
            rt.block_on(async {
                let options = MappingOptions::default();
                let result = engine.map_fields(black_box(&fields), &options).await;
                black_box(result)
            })
        });
    });
}

// Benchmark with caching enabled vs disabled
fn bench_caching_impact(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("caching_impact");

    // With caching
    group.bench_function("with_cache", |b| {
        let config = UnifiedMappingConfig::default();
        let engine = rt.block_on(async {
            UnifiedOntologyMappingEngine::new(config, None, None)
                .await
                .unwrap()
        });
        let field = create_email_field("cache_bench", "customer_email");

        b.iter(|| {
            rt.block_on(async {
                let options = MappingOptions {
                    use_cache: true,
                    ..Default::default()
                };
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    // Without caching (new engine each time)
    group.bench_function("without_cache", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = UnifiedMappingConfig::default();
                let engine = UnifiedOntologyMappingEngine::new(config, None, None)
                    .await
                    .unwrap();
                let field = create_email_field("no_cache_bench", "customer_email");
                let options = MappingOptions {
                    use_cache: false,
                    ..Default::default()
                };
                let result = engine.map_field(black_box(&field), &options).await;
                black_box(result)
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_field_cold_cache,
    bench_single_field_warm_cache,
    bench_field_types,
    bench_batch_mapping,
    bench_confidence_thresholds,
    bench_max_candidates,
    bench_sample_value_counts,
    bench_individual_strategies,
    bench_realistic_workload,
    bench_caching_impact,
);

criterion_main!(benches);
