use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use graphica::governance::{GovernanceBrain, SharedGovernanceBrain};
use graphica::governance::async_brain::AsyncGovernanceBrain;
use graphica::governance::async_config::AsyncGovernanceConfig;
use graphica::governance::rdf_store::GraphicaRdfStore;
use graphica::core::lineage::{LineageEvent, DataRef, TransformRef, ModelRef, CdcPosition, ModelMetrics};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use chrono::Utc;
use uuid::Uuid;

/// Generate a realistic lineage event for benchmarking
fn generate_lineage_event(id: usize) -> LineageEvent {
    LineageEvent {
        id: Uuid::new_v4(),
        dataset: format!("dataset_{}", id % 10),
        record_id: format!("record_{}", id),
        source_refs: vec![
            DataRef {
                system: "kafka".to_string(),
                path: format!("topic_{}/partition_{}/offset_{}", id % 5, id % 3, id),
                version: Some("1.0.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(CdcPosition {
                    topic: format!("topic_{}", id % 5),
                    partition: (id % 3) as i32,
                    offset: id as i64,
                    lsn: None,
                }),
            }
        ],
        transforms: vec![
            TransformRef {
                id: Uuid::new_v4(),
                transform_type: "standardize".to_string(),
                rule_id: "std_001".to_string(),
                version: "1.0.0".to_string(),
                parameters: HashMap::new(),
                applied_at: Utc::now(),
                fields_modified: vec!["field_a".to_string(), "field_b".to_string()],
            },
            TransformRef {
                id: Uuid::new_v4(),
                transform_type: "deduplicate".to_string(),
                rule_id: "dedup_001".to_string(),
                version: "1.0.0".to_string(),
                parameters: HashMap::new(),
                applied_at: Utc::now(),
                fields_modified: vec!["key".to_string()],
            },
        ],
        model_refs: if id % 5 == 0 {
            vec![ModelRef {
                model_id: format!("model_{}", id % 3),
                version: "1.0.0".to_string(),
                model_type: "sklearn.RandomForest".to_string(),
                params_hash: format!("{:x}", id * 3),
                training_data: vec![],
                metrics: ModelMetrics {
                    accuracy: Some(0.95),
                    precision: Some(0.93),
                    recall: Some(0.92),
                    f1_score: Some(0.925),
                    rmse: None,
                    custom_metrics: HashMap::new(),
                },
                registry_uri: format!("mlflow://models/model_{}", id % 3),
                inference_at: Utc::now(),
                features_used: vec!["feature_1".to_string(), "feature_2".to_string()],
                outputs: vec!["prediction".to_string()],
            }]
        } else {
            vec![]
        },
        output_ref: DataRef {
            system: "warehouse".to_string(),
            path: format!("table_{}/record_{}", id % 10, id),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        },
        ts: Utc::now(),
        run_id: format!("run_{}", id / 100),
        tenant_id: "bench_tenant".to_string(),
        correlation_id: Some(format!("corr_{}", id)),
        metadata: HashMap::new(),
    }
}

/// Benchmark synchronous governance brain throughput
fn bench_sync_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/sync");

    // Configure throughput reporting
    group.throughput(Throughput::Elements(1));

    // Test different event counts
    for num_events in [100, 1000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_events),
            num_events,
            |b, &num_events| {
                // Setup
                let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
                    .expect("Failed to create governance brain");

                // Pre-generate events to avoid measurement overhead
                let events: Vec<LineageEvent> = (0..num_events)
                    .map(|i| generate_lineage_event(i))
                    .collect();

                b.iter(|| {
                    for event in &events {
                        brain.insert_lineage_triple(
                            &format!("event:{}", event.id),
                            "rdf:type",
                            "graphica:LineageEvent"
                        ).expect("Failed to insert triple");
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark asynchronous governance brain throughput
fn bench_async_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/async");

    // Configure throughput reporting
    group.throughput(Throughput::Elements(1));

    // Create runtime for async benchmarks
    let runtime = Runtime::new().unwrap();

    // Test different configurations
    let configs = vec![
        ("low_latency", AsyncGovernanceConfig::low_latency()),
        ("balanced", AsyncGovernanceConfig::default()),
        ("high_throughput", AsyncGovernanceConfig::high_throughput()),
    ];

    for (name, config) in configs {
        for num_events in [100, 1000, 10000].iter() {
            group.bench_with_input(
                BenchmarkId::new(name, num_events),
                num_events,
                |b, &num_events| {
                    // Pre-generate events
                    let events: Vec<LineageEvent> = (0..num_events)
                        .map(|i| generate_lineage_event(i))
                        .collect();

                    b.to_async(&runtime).iter(|| async {
                        // Create new brain for each iteration
                        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                        let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                            .expect("Failed to create store");
                        let brain = AsyncGovernanceBrain::new(store, config.clone()).await
                            .expect("Failed to create async brain");

                        // Insert all events
                        for event in &events {
                            brain.materialize_lineage_event(event.clone()).await
                                .expect("Failed to insert event");
                        }

                        // Ensure all events are processed
                        brain.flush().await.expect("Failed to flush");
                        brain.shutdown().await.expect("Failed to shutdown");
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark multi-producer scenarios
fn bench_multi_producer(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/multi_producer");

    group.throughput(Throughput::Elements(1000)); // Total events

    let runtime = Runtime::new().unwrap();

    for num_producers in [1, 2, 4, 8, 16].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_producers),
            num_producers,
            |b, &num_producers| {
                let events_per_producer = 1000 / num_producers;

                b.to_async(&runtime).iter(|| async {
                    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                    let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                        .expect("Failed to create store");
                    let brain = AsyncGovernanceBrain::new(store, AsyncGovernanceConfig::default()).await
                        .expect("Failed to create async brain");

                    // Spawn producer tasks (note: AsyncGovernanceBrain is not Clone)
                    // We'll use a simpler sequential approach for the benchmark
                    for producer_id in 0..num_producers {
                        for i in 0..events_per_producer {
                            let event = generate_lineage_event(producer_id * 1000 + i);
                            brain.materialize_lineage_event(event).await
                                .expect("Failed to insert event");
                        }
                    }

                    brain.flush().await.expect("Failed to flush");
                    brain.shutdown().await.expect("Failed to shutdown");
                });
            },
        );
    }

    group.finish();
}

/// Benchmark batch processing efficiency
fn bench_batch_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/batch_size");

    group.throughput(Throughput::Elements(1000));

    let runtime = Runtime::new().unwrap();

    for batch_size in [1, 10, 50, 100, 500, 1000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                let mut config = AsyncGovernanceConfig::default();
                config.batch_size = batch_size;

                let events: Vec<LineageEvent> = (0..1000)
                    .map(|i| generate_lineage_event(i))
                    .collect();

                b.to_async(&runtime).iter(|| async {
                    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                    let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                        .expect("Failed to create store");
                    let brain = AsyncGovernanceBrain::new(store, config.clone()).await
                        .expect("Failed to create async brain");

                    for event in &events {
                        brain.materialize_lineage_event(event.clone()).await
                            .expect("Failed to insert event");
                    }

                    brain.flush().await.expect("Failed to flush");
                    brain.shutdown().await.expect("Failed to shutdown");
                });
            },
        );
    }

    group.finish();
}

/// Benchmark SharedGovernanceBrain (compatibility layer)
fn bench_shared_brain(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/shared");

    group.throughput(Throughput::Elements(1));

    let _runtime = Runtime::new().unwrap();

    for num_events in [100, 1000, 10000].iter() {
        // Sync API
        group.bench_with_input(
            BenchmarkId::new("sync_api", num_events),
            num_events,
            |b, &num_events| {
                let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
                    .expect("Failed to create governance brain");
                let shared_brain = SharedGovernanceBrain::new(brain);
                let events: Vec<LineageEvent> = (0..num_events)
                    .map(|i| generate_lineage_event(i))
                    .collect();

                b.iter(|| {
                    for event in &events {
                        shared_brain.materialize_event(event)
                            .expect("Failed to insert event");
                    }
                });
            },
        );

        // Async API (if implemented)
        // Skipped for now as SharedGovernanceBrain may not have async API yet
    }

    group.finish();
}

/// Compare sync vs async side-by-side
fn bench_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_throughput/comparison");

    group.throughput(Throughput::Elements(10000));

    let runtime = Runtime::new().unwrap();
    let events: Vec<LineageEvent> = (0..10000)
        .map(|i| generate_lineage_event(i))
        .collect();

    // Synchronous baseline
    group.bench_function("sync_baseline", |b| {
        b.iter(|| {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
                .expect("Failed to create governance brain");

            for event in &events {
                brain.insert_lineage_triple(
                    &format!("event:{}", event.id),
                    "rdf:type",
                    "graphica:LineageEvent"
                ).expect("Failed to insert triple");
            }
        });
    });

    // Asynchronous improved
    group.bench_function("async_improved", |b| {
        b.to_async(&runtime).iter(|| async {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                .expect("Failed to create store");
            let brain = AsyncGovernanceBrain::new(store, AsyncGovernanceConfig::high_throughput()).await
                .expect("Failed to create async brain");

            for event in &events {
                brain.materialize_lineage_event(event.clone()).await
                    .expect("Failed to insert event");
            }

            brain.flush().await.expect("Failed to flush");
            brain.shutdown().await.expect("Failed to shutdown");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sync_throughput,
    bench_async_throughput,
    bench_multi_producer,
    bench_batch_sizes,
    bench_shared_brain,
    bench_comparison
);

criterion_main!(benches);