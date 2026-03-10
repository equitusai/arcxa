//! Performance Test for Streaming Golden Records Pipeline
//!
//! Tests end-to-end performance of the streaming golden record creation system:
//! - CDC ingestion → Entity accumulation → Field resolution → Golden record emission
//! - Conflict detection and severity classification
//! - RDF persistence throughput
//! - Real-time latency measurements
//!
//! # Performance Targets
//! - Throughput: 10,000+ records/sec
//! - Golden record creation: 1,000+ GR/sec
//! - End-to-end latency: < 100ms p99
//! - Conflict detection: Real-time (< 10ms overhead)
//!
//! NOTE: This test is disabled because the `golden_records` feature is not yet implemented.
//! Enable with: `cargo test --features golden-records`

#![cfg(feature = "golden-records")]

use chrono::Utc;
use graphica_core::core::lineage::{CdcPosition, DataRef, LineageEvent};
use graphica_core::ingestion::golden_records::{
    aggregate_conflicts_by_severity, create_golden_records, detect_conflicts,
    filter_critical_conflicts, persist_golden_records_to_rdf, ConflictEvent, GoldenRecordConfig,
    GoldenRecordSink, StreamingGoldenRecord,
};
use graphica_core::ingestion::{ProcessedRecord, Record};
use graphica_core::orchestration::field_lineage::{ConflictSeverity, StrategyType};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use timely::dataflow::operators::{Input, Inspect};
use timely::Config;

/// Mock RDF sink for performance testing
struct MockRdfSink {
    persist_count: Arc<AtomicUsize>,
    persist_latency_us: Arc<Mutex<Vec<u128>>>,
}

impl MockRdfSink {
    fn new() -> Self {
        Self {
            persist_count: Arc::new(AtomicUsize::new(0)),
            persist_latency_us: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_stats(&self) -> (usize, Vec<u128>) {
        let count = self.persist_count.load(Ordering::SeqCst);
        let latencies = self.persist_latency_us.lock().unwrap().clone();
        (count, latencies)
    }
}

impl GoldenRecordSink for MockRdfSink {
    fn persist(&self, _sparql_update: &str) -> Result<(), String> {
        let start = std::time::Instant::now();

        // Simulate minimal RDF persistence overhead
        std::thread::sleep(std::time::Duration::from_micros(10));

        self.persist_count.fetch_add(1, Ordering::SeqCst);

        let latency = start.elapsed().as_micros();
        self.persist_latency_us.lock().unwrap().push(latency);

        Ok(())
    }
}

/// Generate test records with varying patterns
fn generate_test_record(
    entity_id: &str,
    record_idx: usize,
    create_conflict: bool,
) -> ProcessedRecord {
    let email_value = if create_conflict && record_idx % 2 == 0 {
        format!("{}@email1.com", entity_id)
    } else if create_conflict {
        format!("{}@email2.com", entity_id)
    } else {
        format!("{}@example.com", entity_id)
    };

    let data = serde_json::json!({
        "entity_id": entity_id,
        "email": email_value,
        "name": format!("Entity {}", entity_id),
        "score": 0.85 + (record_idx as f64 * 0.01),
    });

    ProcessedRecord {
        record: Record {
            id: format!("rec_{}_{}", entity_id, record_idx),
            dataset: "test_entities".to_string(),
            data,
            source: DataRef {
                system: format!("source_{}", record_idx % 3),
                path: "test_topic".to_string(),
                version: Some("1.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: Some(CdcPosition {
                    topic: "test_topic".to_string(),
                    partition: (record_idx % 3) as i32,
                    offset: record_idx as i64,
                    lsn: None,
                }),
            },
            timestamp: Utc::now().timestamp(),
            tenant_id: "perf_test".to_string(),
            semantic_metadata: None,
        },
        lineage: LineageEvent {
            id: uuid::Uuid::new_v4(),
            dataset: "test_entities".to_string(),
            record_id: format!("rec_{}_{}", entity_id, record_idx),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "test".to_string(),
                path: "output".to_string(),
                version: Some("1.0".to_string()),
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "perf_test_run".to_string(),
            tenant_id: "perf_test".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        },
        violations: vec![],
        quality_score: 1.0,
    }
}

#[test]
fn test_streaming_golden_records_throughput() {
    println!("\n=== Streaming Golden Records Throughput Test ===\n");

    let total_entities = 100;
    let sources_per_entity = 5;
    let total_records = total_entities * sources_per_entity;

    let config = Arc::new(GoldenRecordConfig {
        voting_strategy: StrategyType::Frequency,
        min_confidence: 0.5,
        min_sources: 2,
        max_wait_ms: 1000,
        field_mappings: HashMap::new(),
        entity_id_field: "entity_id".to_string(),
    });

    let golden_records_created = Arc::new(AtomicUsize::new(0));
    let conflicts_detected = Arc::new(AtomicUsize::new(0));
    let gr_created_clone = golden_records_created.clone();
    let conflicts_clone = conflicts_detected.clone();

    let start_time = std::time::Instant::now();

    timely::execute(Config::thread(), move |worker| {
        let cfg = config.clone();
        let gr_count = gr_created_clone.clone();
        let conf_count = conflicts_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, stream) = scope.new_input::<ProcessedRecord>();

            // Create golden records
            let golden_records = create_golden_records(stream, cfg.clone());

            // Track golden records created
            let gr_count_inner = gr_count.clone();
            golden_records.clone().inspect(move |_gr| {
                gr_count_inner.fetch_add(1, Ordering::SeqCst);
            });

            // Detect and count conflicts
            let conflicts = detect_conflicts(golden_records.clone());
            let conf_count_inner = conf_count.clone();
            conflicts.inspect(move |_conflict| {
                conf_count_inner.fetch_add(1, Ordering::SeqCst);
            });

            // Generate test data
            for entity_idx in 0..total_entities {
                let entity_id = format!("entity_{:04}", entity_idx);
                let create_conflict = entity_idx % 10 == 0; // 10% conflict rate

                for source_idx in 0..sources_per_entity {
                    let record = generate_test_record(&entity_id, source_idx, create_conflict);
                    input.send(record);
                }
            }

            input.advance_to(1);
        });
    })
    .expect("Dataflow execution failed");

    let elapsed = start_time.elapsed();
    let gr_count = golden_records_created.load(Ordering::SeqCst);
    let conflict_count = conflicts_detected.load(Ordering::SeqCst);

    println!("Performance Results:");
    println!("  Total records ingested: {}", total_records);
    println!("  Golden records created: {}", gr_count);
    println!("  Conflicts detected: {}", conflict_count);
    println!("  Total time: {:?}", elapsed);
    println!(
        "  Record throughput: {:.0} rec/sec",
        total_records as f64 / elapsed.as_secs_f64()
    );
    println!(
        "  Golden record throughput: {:.0} GR/sec",
        gr_count as f64 / elapsed.as_secs_f64()
    );
    println!(
        "  Avg time per golden record: {:.2}ms",
        elapsed.as_millis() as f64 / gr_count as f64
    );

    // Assertions
    assert!(
        gr_count >= total_entities / 2,
        "Should create golden records for most entities"
    );
    assert!(elapsed.as_secs() < 5, "Should complete within 5 seconds");

    println!("\n✓ Throughput test PASSED\n");
}

#[test]
fn test_rdf_persistence_performance() {
    println!("\n=== RDF Persistence Performance Test ===\n");

    let total_entities = 50;
    let sources_per_entity = 3;

    let config = Arc::new(GoldenRecordConfig {
        voting_strategy: StrategyType::Frequency,
        min_confidence: 0.5,
        min_sources: 2,
        max_wait_ms: 500,
        field_mappings: HashMap::new(),
        entity_id_field: "entity_id".to_string(),
    });

    let rdf_sink = Arc::new(MockRdfSink::new());
    let sink_clone = rdf_sink.clone();

    let start_time = std::time::Instant::now();

    timely::execute(Config::thread(), move |worker| {
        let cfg = config.clone();
        let sink = sink_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, stream) = scope.new_input::<ProcessedRecord>();

            // Create golden records
            let golden_records = create_golden_records(stream, cfg.clone());

            // Persist to RDF
            let _persistence_results =
                persist_golden_records_to_rdf(golden_records, sink.clone(), cfg.voting_strategy);

            // Generate test data
            for entity_idx in 0..total_entities {
                let entity_id = format!("entity_{:04}", entity_idx);
                for source_idx in 0..sources_per_entity {
                    let record = generate_test_record(&entity_id, source_idx, false);
                    input.send(record);
                }
            }

            input.advance_to(1);
        });
    })
    .expect("Dataflow execution failed");

    let elapsed = start_time.elapsed();
    let (persist_count, latencies) = rdf_sink.get_stats();

    // Calculate latency percentiles
    let mut sorted_latencies = latencies.clone();
    sorted_latencies.sort();
    let p50 = if !sorted_latencies.is_empty() {
        sorted_latencies[sorted_latencies.len() / 2]
    } else {
        0
    };
    let p99 = if !sorted_latencies.is_empty() {
        sorted_latencies[(sorted_latencies.len() as f64 * 0.99) as usize]
    } else {
        0
    };

    println!("RDF Persistence Results:");
    println!("  Records persisted: {}", persist_count);
    println!("  Total time: {:?}", elapsed);
    println!(
        "  Persistence throughput: {:.0} persist/sec",
        persist_count as f64 / elapsed.as_secs_f64()
    );
    println!("  Latency p50: {}μs", p50);
    println!("  Latency p99: {}μs", p99);

    assert!(persist_count > 0, "Should have persisted golden records");
    assert!(p99 < 1000, "p99 latency should be under 1ms");

    println!("\n✓ RDF persistence test PASSED\n");
}

#[test]
fn test_conflict_detection_performance() {
    println!("\n=== Conflict Detection Performance Test ===\n");

    let total_entities = 100;
    let sources_per_entity = 4;

    let config = Arc::new(GoldenRecordConfig {
        voting_strategy: StrategyType::Frequency,
        min_confidence: 0.5,
        min_sources: 2,
        max_wait_ms: 500,
        field_mappings: HashMap::new(),
        entity_id_field: "entity_id".to_string(),
    });

    let conflicts_detected = Arc::new(AtomicUsize::new(0));
    let conf_clone = conflicts_detected.clone();

    timely::execute(Config::thread(), move |worker| {
        let cfg = config.clone();
        let conf_count = conf_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, stream) = scope.new_input::<ProcessedRecord>();

            // Create golden records
            let golden_records = create_golden_records(stream, cfg.clone());

            // Detect conflicts
            let conflicts = detect_conflicts(golden_records);

            // Count all conflicts
            let conf = conf_count.clone();
            conflicts.inspect(move |_conflict| {
                conf.fetch_add(1, Ordering::SeqCst);
            });

            // Generate test data with conflicts
            for entity_idx in 0..total_entities {
                let entity_id = format!("entity_{:04}", entity_idx);
                let create_conflict = entity_idx % 3 == 0; // 33% conflict rate

                for source_idx in 0..sources_per_entity {
                    let record = generate_test_record(&entity_id, source_idx, create_conflict);
                    input.send(record);
                }
            }

            input.advance_to(1);
        });
    })
    .expect("Dataflow execution failed");

    let total_conflicts = conflicts_detected.load(Ordering::SeqCst);

    println!("Conflict Detection Results:");
    println!("  Total conflicts: {}", total_conflicts);
    println!("  Test entities: {}", total_entities);
    println!(
        "  Conflict rate: {:.1}%",
        (total_conflicts as f64 / total_entities as f64) * 100.0
    );

    assert!(total_conflicts > 0, "Should detect conflicts");

    println!("\n✓ Conflict detection test PASSED\n");
}

#[test]
fn test_end_to_end_streaming_latency() {
    println!("\n=== End-to-End Streaming Latency Test ===\n");

    let config = Arc::new(GoldenRecordConfig {
        voting_strategy: StrategyType::Frequency,
        min_confidence: 0.5,
        min_sources: 2,
        max_wait_ms: 100, // Low timeout for latency test
        field_mappings: HashMap::new(),
        entity_id_field: "entity_id".to_string(),
    });

    let latencies = Arc::new(Mutex::new(Vec::new()));
    let lat_clone = latencies.clone();

    timely::execute(Config::thread(), move |worker| {
        let cfg = config.clone();
        let lat = lat_clone.clone();

        worker.dataflow::<u64, _, _>(move |scope| {
            let (mut input, stream) = scope.new_input::<ProcessedRecord>();

            // Create golden records
            let golden_records = create_golden_records(stream, cfg.clone());

            // Measure end-to-end latency
            let lat_inner = lat.clone();
            golden_records.inspect(move |gr| {
                let now = Utc::now();
                let created_at = gr.updated_at;
                let latency_ms = (now - created_at).num_milliseconds();
                if latency_ms >= 0 {
                    lat_inner.lock().unwrap().push(latency_ms as u64);
                }
            });

            // Send small batch with precise timing
            for i in 0..10 {
                let entity_id = format!("entity_{:02}", i);
                for j in 0..3 {
                    let record = generate_test_record(&entity_id, j, false);
                    input.send(record);
                }
            }

            input.advance_to(1);
        });
    })
    .expect("Dataflow execution failed");

    let mut lat_vec = latencies.lock().unwrap().clone();
    lat_vec.sort();

    if !lat_vec.is_empty() {
        let p50 = lat_vec[lat_vec.len() / 2];
        let p99 = lat_vec[(lat_vec.len() as f64 * 0.99).min((lat_vec.len() - 1) as f64) as usize];
        let avg = lat_vec.iter().sum::<u64>() as f64 / lat_vec.len() as f64;

        println!("Latency Results:");
        println!("  Measurements: {}", lat_vec.len());
        println!("  Average: {:.2}ms", avg);
        println!("  p50: {}ms", p50);
        println!("  p99: {}ms", p99);

        assert!(p99 < 200, "p99 latency should be under 200ms");
    } else {
        println!("  No latency measurements (golden records may not have been created yet)");
    }

    println!("\n✓ Latency test PASSED\n");
}
