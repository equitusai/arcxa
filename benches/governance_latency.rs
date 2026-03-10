use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use graphica::governance::GovernanceBrain;
use graphica::governance::async_brain::AsyncGovernanceBrain;
use graphica::governance::async_config::AsyncGovernanceConfig;
use graphica::governance::rdf_store::GraphicaRdfStore;
use graphica::core::lineage::{LineageEvent, DataRef, CdcPosition};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::runtime::Runtime;
use hdrhistogram::Histogram;
use chrono::Utc;
use uuid::Uuid;

/// Generate a simple lineage event
fn generate_event(id: usize) -> LineageEvent {
    LineageEvent {
        id: Uuid::new_v4(),
        dataset: "test_dataset".to_string(),
        record_id: format!("record_{}", id),
        source_refs: vec![
            DataRef {
                system: "kafka".to_string(),
                path: format!("topic/partition/offset_{}", id),
                version: Some("1.0.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(CdcPosition {
                    topic: "test_topic".to_string(),
                    partition: 0,
                    offset: id as i64,
                    lsn: None,
                }),
            }
        ],
        transforms: vec![],
        model_refs: vec![],
        output_ref: DataRef {
            system: "warehouse".to_string(),
            path: format!("table/record_{}", id),
            version: None,
            extracted_at: Utc::now(),
            cdc_position: None,
        },
        ts: Utc::now(),
        run_id: "test_run".to_string(),
        tenant_id: "test_tenant".to_string(),
        correlation_id: Some(format!("corr_{}", id)),
        metadata: HashMap::new(),
    }
}

/// Measure latency distribution for sync brain
fn bench_sync_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_latency/sync");

    group.bench_function("insert_event_p99", |b| {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
            .expect("Failed to create governance brain");

        let mut histogram = Histogram::<u64>::new(3).unwrap();
        let mut event_id = 0;

        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let event = generate_event(event_id);
                event_id += 1;

                let start = Instant::now();
                brain.insert_lineage_triple(
                    &format!("event:{}", event.id),
                    "rdf:type",
                    "graphica:LineageEvent"
                ).expect("Failed to insert triple");
                let elapsed = start.elapsed();

                histogram.record(elapsed.as_micros() as u64).unwrap();
                total += elapsed;
            }

            total
        });

        // Report percentiles
        println!("Sync Latency Percentiles (μs):");
        println!("  P50:  {}", histogram.value_at_percentile(50.0));
        println!("  P95:  {}", histogram.value_at_percentile(95.0));
        println!("  P99:  {}", histogram.value_at_percentile(99.0));
        println!("  P99.9: {}", histogram.value_at_percentile(99.9));
        println!("  Max:  {}", histogram.max());
    });

    group.bench_function("query_lineage_p99", |b| {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
            .expect("Failed to create governance brain");

        // Pre-populate with events
        for i in 0..10000 {
            let event = generate_event(i);
            brain.insert_lineage_triple(
                &format!("event:{}", event.id),
                "rdf:type",
                "graphica:LineageEvent"
            ).expect("Failed to insert triple");
        }

        let mut histogram = Histogram::<u64>::new(3).unwrap();

        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _i in 0..iters {
                let query = "SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1";

                let start = Instant::now();
                let _ = brain.query(query);
                let elapsed = start.elapsed();

                histogram.record(elapsed.as_micros() as u64).unwrap();
                total += elapsed;
            }

            total
        });

        println!("Sync Query Latency Percentiles (μs):");
        println!("  P50:  {}", histogram.value_at_percentile(50.0));
        println!("  P95:  {}", histogram.value_at_percentile(95.0));
        println!("  P99:  {}", histogram.value_at_percentile(99.0));
    });

    group.finish();
}

/// Measure latency distribution for async brain
fn bench_async_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_latency/async");

    let runtime = Runtime::new().unwrap();

    // Test different configurations
    let configs = vec![
        ("low_latency", AsyncGovernanceConfig::low_latency()),
        ("balanced", AsyncGovernanceConfig::default()),
        ("high_throughput", AsyncGovernanceConfig::high_throughput()),
    ];

    for (name, config) in configs {
        group.bench_function(
            BenchmarkId::new("insert_event_p99", name),
            |b| {
                b.to_async(&runtime).iter(|| {
                    let config = config.clone();
                    async move {
                        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
                        let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                            .expect("Failed to create store");
                        let brain = AsyncGovernanceBrain::new(store, config).await
                            .expect("Failed to create async brain");

                        let event = generate_event(0);

                        brain.materialize_lineage_event(event).await
                            .expect("Failed to insert event");

                        brain.flush().await.expect("Failed to flush");
                        brain.shutdown().await.expect("Failed to shutdown");
                    }
                });
            }
        );
    }

    group.finish();
}

/// Measure query latency under load
fn bench_query_under_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_latency/query_under_load");

    let runtime = Runtime::new().unwrap();

    group.bench_function("query_basic", |b| {
        b.to_async(&runtime).iter(|| async {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                .expect("Failed to create store");
            let brain = AsyncGovernanceBrain::new(store, AsyncGovernanceConfig::default()).await
                .expect("Failed to create async brain");

            // Pre-populate
            for i in 0..100 {
                brain.materialize_lineage_event(generate_event(i)).await
                    .expect("Failed to insert event");
            }

            // Execute a simple query
            let _result = brain.query("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 10").await;

            brain.shutdown().await.expect("Failed to shutdown");
        });
    });

    group.finish();
}

/// Measure end-to-end latency (insert + query)
fn bench_end_to_end_latency(c: &mut Criterion) {
    let mut group = c.benchmark_group("governance_latency/end_to_end");

    let runtime = Runtime::new().unwrap();

    group.bench_function("sync_e2e", |b| {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let brain = GovernanceBrain::new(temp_dir.path().to_str().unwrap())
            .expect("Failed to create governance brain");

        let mut histogram = Histogram::<u64>::new(3).unwrap();
        let mut event_id = 0;

        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;

            for _ in 0..iters {
                let event = generate_event(event_id);
                event_id += 1;

                let start = Instant::now();

                // Insert
                brain.insert_lineage_triple(
                    &format!("event:{}", event.id),
                    "rdf:type",
                    "graphica:LineageEvent"
                ).expect("Failed to insert triple");

                // Query immediately
                let _ = brain.query("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1");

                let elapsed = start.elapsed();

                histogram.record(elapsed.as_micros() as u64).unwrap();
                total += elapsed;
            }

            total
        });

        println!("Sync E2E Latency (μs):");
        println!("  P99: {}", histogram.value_at_percentile(99.0));
    });

    group.bench_function("async_e2e", |b| {
        b.to_async(&runtime).iter(|| async {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let store = GraphicaRdfStore::new(temp_dir.path().to_str().unwrap())
                .expect("Failed to create store");
            let brain = AsyncGovernanceBrain::new(store, AsyncGovernanceConfig::low_latency()).await
                .expect("Failed to create async brain");

            let event = generate_event(0);

            // Insert
            brain.materialize_lineage_event(event).await
                .expect("Failed to insert event");

            // Query immediately
            let _ = brain.query("SELECT ?s ?p ?o WHERE { ?s ?p ?o } LIMIT 1").await;

            brain.shutdown().await.expect("Failed to shutdown");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_sync_latency,
    bench_async_latency,
    bench_query_under_load,
    bench_end_to_end_latency
);

criterion_main!(benches);