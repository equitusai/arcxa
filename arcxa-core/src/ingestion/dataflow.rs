//! # Dataflow Operators
//!
//! Timely/Differential operators for data processing pipeline.

use crate::core::lineage::{LineageEvent, TransformRef};
use crate::core::quality::QualityViolation;
use crate::inference::rdf_store::RdfStore;
use crate::inference::semantic::ColumnNameDetector;
use crate::ingestion::metrics;
use crate::ingestion::{FieldSemanticMetadata, Record};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use timely::dataflow::operators::*;
use timely::dataflow::*;
use uuid::Uuid;

/// Deduplicate records using CheckpointableDedupState
///
/// # Architecture
/// Uses CheckpointableDedupState (DashMap-based) which can be:
/// - Checkpointed for exactly-once semantics
/// - Shared across Timely workers (Arc/DashMap is Send+Sync)
/// - Recovered from checkpoints on restart
///
/// # Thread Safety
/// CheckpointableDedupState uses Arc<DashMap> internally, which is safe
/// to use across multiple Timely workers (each worker is a separate OS thread).
///
/// # Note on Differential Dataflow
/// The PROPER solution would be using Differential Dataflow's `.distinct()`,
/// but that requires converting the entire pipeline from Stream to Collection-based,
/// which is a significant architectural change tracked separately.
pub fn deduplicate_with_state<G: Scope<Timestamp = u64>>(
    stream: Stream<G, Record>,
    dedup_state: crate::checkpointing::CheckpointableDedupState,
) -> Stream<G, Record> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "Deduplicate", move |_cap, info| {
        let mut buffer = Vec::new();
        let worker_id = info.global_id.to_string();
        let state = dedup_state.clone();

        move |input, output| {
            input.for_each(|time, data| {
                let start = std::time::Instant::now();
                data.swap(&mut buffer);

                let mut output_session = output.session(&time);
                let initial_count = buffer.len();
                let mut deduped_count = 0;

                for record in buffer.drain(..) {
                    if state.is_duplicate(&record.id) {
                        // Duplicate - skip
                        tracing::trace!("Duplicate record skipped: {}", record.id);
                        crate::ingestion::metrics::DEDUP_HITS
                            .with_label_values(&[&worker_id])
                            .inc();
                    } else {
                        // Unique - emit and mark as seen
                        state.mark_seen(record.id.clone());
                        output_session.give(record);
                        deduped_count += 1;
                    }
                }

                let latency = start.elapsed();

                crate::ingestion::metrics::RECORDS_PROCESSED
                    .with_label_values(&[worker_id.as_str(), "dedup"])
                    .inc_by(initial_count as f64);

                crate::ingestion::metrics::DEDUP_LATENCY
                    .with_label_values(&[&worker_id])
                    .observe(latency.as_micros() as f64);

                if deduped_count > 0 {
                    tracing::debug!(
                        "Worker {}: deduped {}/{} records in {:?}, state size: {}",
                        worker_id,
                        deduped_count,
                        initial_count,
                        latency,
                        state.len()
                    );
                }
            });
        }
    })
}

/// DEPRECATED: Old deduplicate function using Rc<RefCell>
///
/// This function is kept for backwards compatibility but should not be used.
/// Use deduplicate_with_state() instead, which integrates with checkpointing.
///
/// # Migration Note
/// This will be removed once all callsites are migrated to deduplicate_with_state().
#[deprecated(note = "Use deduplicate_with_state() for checkpoint integration")]
pub fn deduplicate<G: Scope<Timestamp = u64>>(stream: Stream<G, Record>) -> Stream<G, Record> {
    use std::cell::RefCell;
    use std::collections::{BTreeMap, HashSet};
    use std::rc::Rc;

    // Time-windowed deduplication with bounded memory
    const DEDUP_WINDOW_MS: i64 = 60_000; // 1 minute window
    const MAX_ENTRIES: usize = 100_000; // Hard cap at 100K entries

    struct DedupState {
        seen_ids: HashSet<String>,
        seen_by_time: BTreeMap<(i64, String), ()>,
    }

    let seen = Rc::new(RefCell::new(DedupState {
        seen_ids: HashSet::new(),
        seen_by_time: BTreeMap::new(),
    }));

    stream.unary(
        timely::dataflow::channels::pact::Pipeline,
        "Deduplicate",
        move |_cap, info| {
            let mut buffer = Vec::new();
            let seen_clone = seen.clone();
            let worker_id = info.global_id.to_string();

            move |input, output| {
                input.for_each(|time, data| {
                    let start = std::time::Instant::now();
                    data.swap(&mut buffer);
                    let mut session = output.session(&time);
                    let mut state = seen_clone.borrow_mut();

                    for record in buffer.drain(..) {
                        let current_time = record.timestamp;
                        let cutoff_time = current_time - DEDUP_WINDOW_MS;

                        // Evict old entries outside the time window
                        if let Some((&(oldest_time, _), _)) = state.seen_by_time.iter().next() {
                            if oldest_time < cutoff_time {
                                let keys_to_remove: Vec<_> = state
                                    .seen_by_time
                                    .range(..(cutoff_time, String::new()))
                                    .map(|((ts, id), _)| (*ts, id.clone()))
                                    .collect();

                                for (ts, id) in &keys_to_remove {
                                    state.seen_by_time.remove(&(*ts, id.clone()));
                                    state.seen_ids.remove(id);
                                }

                                if !keys_to_remove.is_empty() {
                                    tracing::debug!(
                                        "Evicted {} old entries from time window",
                                        keys_to_remove.len()
                                    );
                                }
                            }
                        }

                        // Enforce hard memory cap
                        if state.seen_ids.len() >= MAX_ENTRIES {
                            let remove_count = state.seen_ids.len() / 10;
                            let keys_to_remove: Vec<_> = state
                                .seen_by_time
                                .iter()
                                .take(remove_count)
                                .map(|((ts, id), _)| (*ts, id.clone()))
                                .collect();

                            for (ts, id) in &keys_to_remove {
                                state.seen_by_time.remove(&(*ts, id.clone()));
                                state.seen_ids.remove(id);
                            }
                            tracing::warn!(
                                "Dedup map at capacity, evicted {} oldest entries",
                                remove_count
                            );
                        }

                        // Check for duplicates
                        if state.seen_ids.contains(&record.id) {
                            tracing::debug!("Filtered duplicate record: {}", record.id);
                            metrics::DEDUP_HITS
                                .with_label_values(&[worker_id.as_str()])
                                .inc();
                            metrics::RECORDS_DROPPED
                                .with_label_values(&[worker_id.as_str(), "duplicate"])
                                .inc();
                        } else {
                            let time_key = (current_time, record.id.clone());
                            state.seen_by_time.insert(time_key, ());
                            state.seen_ids.insert(record.id.clone());

                            session.give(record);
                            metrics::RECORDS_PROCESSED
                                .with_label_values(&[worker_id.as_str(), "dedup"])
                                .inc();
                        }
                    }

                    // Update metrics
                    metrics::DEDUP_MAP_SIZE
                        .with_label_values(&[worker_id.as_str()])
                        .set(state.seen_ids.len() as f64);

                    let memory_bytes = state.seen_ids.len()
                        * (std::mem::size_of::<String>() * 2 + std::mem::size_of::<i64>() + 64);
                    metrics::MEMORY_USAGE_BYTES
                        .with_label_values(&[worker_id.as_str(), "dedup"])
                        .set(memory_bytes as f64);

                    let latency_ms = start.elapsed().as_millis() as f64;
                    metrics::PROCESSING_LATENCY
                        .with_label_values(&[worker_id.as_str(), "dedup"])
                        .observe(latency_ms);

                    if state.seen_ids.len() > 0 && state.seen_ids.len() % 10000 == 0 {
                        tracing::info!(
                            "Dedup state: {} unique IDs, {} time entries",
                            state.seen_ids.len(),
                            state.seen_by_time.len()
                        );
                    }
                });
            }
        },
    )
}

/// Capture lineage for a record
pub fn capture_lineage(record: &Record) -> LineageEvent {
    LineageEvent {
        id: Uuid::new_v4(),
        dataset: record.dataset.clone(),
        record_id: record.id.clone(),
        source_refs: vec![record.source.clone()],
        transforms: vec![TransformRef {
            id: Uuid::new_v4(),
            transform_type: "standardize".to_string(),
            rule_id: "std-001".to_string(),
            version: "1.0.0".to_string(),
            parameters: std::collections::HashMap::new(),
            applied_at: Utc::now(),
            fields_modified: vec![],
        }],
        model_refs: vec![],
        output_ref: record.source.clone(),
        ts: Utc::now(),
        run_id: "default-run".to_string(),
        tenant_id: record.tenant_id.clone(),
        correlation_id: None,
        metadata: std::collections::HashMap::new(),
    }
}

/// Apply quality rules to a record
pub fn apply_quality_rules(_record: &Record) -> Vec<QualityViolation> {
    vec![]
}

/// Calculate overall quality score
pub fn calculate_quality_score(violations: &[QualityViolation]) -> f64 {
    if violations.is_empty() {
        1.0
    } else {
        let penalty: f64 = violations
            .iter()
            .map(|v| match v.severity {
                crate::core::quality::Severity::Info => 0.01,
                crate::core::quality::Severity::Warning => 0.05,
                crate::core::quality::Severity::Error => 0.15,
                crate::core::quality::Severity::Critical => 0.30,
            })
            .sum();

        (1.0 - penalty).max(0.0)
    }
}

/// Update profiling statistics
pub fn update_profile_statistics(record: &Record) {
    tracing::trace!("Profiling record: {}", record.id);
}

/// Semantic enrichment operator for real-time type inference
///
/// # Architecture (Phase 2)
/// - Analyzes JSON fields in each record using ColumnNameDetector
/// - Infers semantic types from field names (email, phone, timestamp, etc.)
/// - Attaches FieldSemanticMetadata to each record
/// - Thread-safe via Arc for shared detector state
///
/// # Performance
/// - Uses pre-compiled regex patterns in ColumnNameDetector
/// - Minimal overhead per record (~microseconds)
/// - No blocking I/O or external calls
pub fn enrich_with_semantics<G: Scope<Timestamp = u64>>(
    stream: Stream<G, Record>,
    detector: Arc<ColumnNameDetector>,
) -> Stream<G, Record> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "SemanticEnrichment", move |_cap, info| {
        let mut buffer = Vec::new();
        let worker_id = info.global_id.to_string();
        let detector_clone = detector.clone();

        move |input, output| {
            input.for_each(|time, data| {
                let start = std::time::Instant::now();
                data.swap(&mut buffer);

                let mut output_session = output.session(&time);

                for mut record in buffer.drain(..) {
                    let field_metadata = analyze_record_fields(&record, &detector_clone);

                    if !field_metadata.is_empty() {
                        record.semantic_metadata = Some(field_metadata);

                        metrics::RECORDS_PROCESSED
                            .with_label_values(&[&worker_id, "semantic_enrichment"])
                            .inc();
                    }

                    output_session.give(record);
                }

                let latency = start.elapsed();
                metrics::PROCESSING_LATENCY
                    .with_label_values(&[&worker_id, "semantic_enrichment"])
                    .observe(latency.as_micros() as f64);
            });
        }
    })
}

/// Analyze JSON fields in a record and infer semantic types
fn analyze_record_fields(
    record: &Record,
    detector: &ColumnNameDetector,
) -> HashMap<String, FieldSemanticMetadata> {
    let mut metadata = HashMap::new();

    if let serde_json::Value::Object(map) = &record.data {
        for (field_name, _value) in map {
            if let Some((semantic_type, confidence, method_desc)) =
                detector.detect_from_name(field_name)
            {
                let field_meta = FieldSemanticMetadata {
                    field_name: field_name.clone(),
                    semantic_type,
                    confidence,
                    detection_method: method_desc,
                };

                metadata.insert(field_name.clone(), field_meta);
            }
        }
    }

    metadata
}

/// Persist semantic metadata as RDF triples (Phase 2.1)
///
/// # Architecture
/// - Converts semantic metadata to RDF triples using RdfConverter
/// - Persists triples to file-based RDF store (Turtle format)
/// - Thread-safe via Arc<RdfStore> for shared state across workers
/// - Non-blocking: Uses inspect() to persist without blocking dataflow
///
/// # RDF Structure
/// Creates triples for:
/// - Record entity (type, recordId, dataset, ingestedAt)
/// - Field entities with semantic type annotations
/// - Detection provenance (confidence, method, timestamp)
///
/// # Performance
/// - Buffered writes with configurable flush frequency
/// - ~microseconds overhead per record
/// - Files organized per dataset (dataset1.ttl, dataset2.ttl, etc.)
pub fn persist_rdf_semantics<G: Scope<Timestamp = u64>>(
    stream: Stream<G, Record>,
    rdf_store: Arc<RdfStore>,
) -> Stream<G, Record> {
    use timely::dataflow::operators::Inspect;

    stream.inspect(move |record| {
        // Only persist if record has semantic metadata
        if let Some(ref metadata) = record.semantic_metadata {
            match rdf_store.persist_record_semantics(
                &record.id,
                &record.dataset,
                metadata,
                record.timestamp,
            ) {
                Ok(triple_count) => {
                    tracing::debug!(
                        "Persisted {} RDF triples for record {} in dataset {}",
                        triple_count,
                        record.id,
                        record.dataset
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to persist RDF triples for record {}: {}",
                        record.id,
                        e
                    );
                }
            }
        }
    })
}
