//! Streaming Resolved Entity Creation
//!
//! Timely dataflow operators for real-time field resolution and entity consolidation.
//!
//! # Architecture
//!
//! ```text
//! CDC Stream → Group by Entity → Accumulate Sources → Resolve Fields → Resolved Entity
//! ```
//!
//! Uses Timely's stateful operators to:
//! - Accumulate source values per entity/field
//! - Trigger resolution when threshold met or timeout occurs
//! - Emit incremental resolved entity updates
//! - Track conflicts in real-time

use crate::ingestion::{ProcessedRecord, Record};
use crate::orchestration::field_lineage::resolver::ResolvedEntity;
use crate::orchestration::field_lineage::{
    ConflictSeverity, FieldResolution, FieldResolver, FieldValue, SourceValue, StrategyType,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use timely::dataflow::operators::*;
use timely::dataflow::*;

/// Trait for persisting resolved entities to RDF stores
pub trait ResolvedEntitySink: Send + Sync {
    /// Persist a resolved entity using SPARQL UPDATE
    fn persist(&self, sparql_update: &str) -> Result<(), String>;
}

/// Trait for caching resolved entities (e.g., in-memory cache, Redis)
pub trait ResolvedEntityCache: Send + Sync {
    /// Insert a resolved entity into the cache
    fn insert(&self, record: StreamingResolvedEntity) -> Result<(), String>;
}

/// Resolved entity with streaming metadata
#[derive(Debug, Clone)]
pub struct StreamingResolvedEntity {
    /// Entity ID
    pub entity_id: String,

    /// Resolved fields
    pub fields: HashMap<String, FieldValue>,

    /// Field resolutions (for lineage)
    pub resolutions: Vec<FieldResolution>,

    /// Overall confidence
    pub overall_confidence: f64,

    /// Number of source records accumulated
    pub source_count: usize,

    /// Conflict count
    pub conflict_count: usize,

    /// Requires human review
    pub requires_review: bool,

    /// Timestamp of golden record creation/update
    pub updated_at: chrono::DateTime<Utc>,
}

impl StreamingResolvedEntity {
    /// Convert to ResolvedEntity for SPARQL generation
    pub fn to_resolved_entity(&self) -> ResolvedEntity {
        ResolvedEntity {
            entity_id: self.entity_id.clone(),
            fields: self.fields.clone(),
            resolutions: self.resolutions.clone(),
            overall_confidence: self.overall_confidence,
            created_at: self.updated_at,
            conflict_count: self.conflict_count,
            requires_review: self.requires_review,
        }
    }
}

/// Configuration for streaming golden record creation
#[derive(Debug, Clone)]
pub struct ResolvedEntityConfig {
    /// Voting strategy to use
    pub voting_strategy: StrategyType,

    /// Minimum confidence threshold
    pub min_confidence: f64,

    /// Minimum source values before resolving
    pub min_sources: usize,

    /// Maximum time to wait for sources (milliseconds)
    pub max_wait_ms: u64,

    /// Field mapping: record field name → entity field name
    pub field_mappings: HashMap<String, String>,

    /// Entity ID extraction function name
    pub entity_id_field: String,
}

impl Default for ResolvedEntityConfig {
    fn default() -> Self {
        Self {
            voting_strategy: StrategyType::Frequency,
            min_confidence: 0.70,
            min_sources: 2,
            max_wait_ms: 5000,
            field_mappings: HashMap::new(),
            entity_id_field: "id".to_string(),
        }
    }
}

/// Accumulated source values for entity resolution
#[derive(Debug, Clone)]
struct EntityAccumulator {
    entity_id: String,
    fields: HashMap<String, Vec<SourceValue>>,
    first_seen: chrono::DateTime<Utc>,
    last_updated: chrono::DateTime<Utc>,
    source_count: usize,
}

impl EntityAccumulator {
    fn new(entity_id: String) -> Self {
        let now = Utc::now();
        Self {
            entity_id,
            fields: HashMap::new(),
            first_seen: now,
            last_updated: now,
            source_count: 0,
        }
    }

    fn add_source_value(&mut self, field_name: String, source: SourceValue) {
        self.fields
            .entry(field_name)
            .or_insert_with(Vec::new)
            .push(source);
        self.last_updated = Utc::now();
        self.source_count += 1;
    }

    fn should_resolve(
        &self,
        config: &ResolvedEntityConfig,
        current_time: chrono::DateTime<Utc>,
    ) -> bool {
        // Check if we have minimum sources
        if self.source_count < config.min_sources {
            return false;
        }

        // Check if timeout has been reached
        let age_ms = (current_time - self.first_seen).num_milliseconds() as u64;
        if age_ms >= config.max_wait_ms {
            return true;
        }

        // Could add more sophisticated triggering logic here
        // e.g., resolve immediately if all expected fields have data

        true
    }
}

/// Extract entity ID from a record
fn extract_entity_id(record: &Record, config: &ResolvedEntityConfig) -> Option<String> {
    // Try to extract from data JSON
    record
        .data
        .get(&config.entity_id_field)
        .and_then(|v| v.as_str())
        .map(String::from)
        .or_else(|| Some(record.id.clone())) // Fallback to record ID
}

/// Convert Record to SourceValues for field resolution
fn record_to_source_values(
    record: &Record,
    config: &ResolvedEntityConfig,
) -> HashMap<String, SourceValue> {
    let mut sources = HashMap::new();

    // Extract fields from record data
    if let Some(obj) = record.data.as_object() {
        for (field_name, value) in obj {
            // Map field name if configured
            let target_field = config
                .field_mappings
                .get(field_name)
                .cloned()
                .unwrap_or_else(|| field_name.clone());

            // Create source value
            let source = SourceValue {
                id: format!("{}_{}", record.id, field_name),
                value: value.clone(),
                source_system: record.dataset.clone(),
                source_timestamp: chrono::DateTime::from_timestamp(record.timestamp, 0)
                    .unwrap_or_else(Utc::now),
                source_authority: 0.8, // Could be configured per dataset
                confidence: None,
                vote_count: 0,
                vote_weight: 1.0,
                metadata: HashMap::new(),
            };

            sources.insert(target_field, source);
        }
    }

    sources
}

/// Streaming operator for golden record creation
///
/// Groups incoming records by entity ID, accumulates source values,
/// and emits golden records when resolution criteria are met.
pub fn create_resolved_entities<G: Scope<Timestamp = u64>>(
    stream: Stream<G, ProcessedRecord>,
    config: Arc<ResolvedEntityConfig>,
) -> Stream<G, StreamingResolvedEntity> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "GoldenRecordCreation", move |_cap, info| {
        let mut buffer = Vec::new();
        let worker_id = info.global_id.to_string();
        let cfg = config.clone();

        // State: entity_id → EntityAccumulator
        let mut accumulators: HashMap<String, EntityAccumulator> = HashMap::new();

        // Resolver for field resolution
        let resolver = FieldResolver::with_strategy(cfg.voting_strategy)
            .with_min_confidence(cfg.min_confidence);

        move |input, output| {
            input.for_each(|time, data| {
                let start = std::time::Instant::now();
                data.swap(&mut buffer);

                let mut output_session = output.session(&time);
                let current_time = Utc::now();

                // Process incoming records
                for processed in buffer.drain(..) {
                    let record = &processed.record;

                    // Extract entity ID
                    let entity_id = match extract_entity_id(record, &cfg) {
                        Some(id) => id,
                        None => {
                            tracing::warn!("No entity ID found for record {}", record.id);
                            continue;
                        }
                    };

                    // Get or create accumulator
                    let accumulator = accumulators
                        .entry(entity_id.clone())
                        .or_insert_with(|| EntityAccumulator::new(entity_id.clone()));

                    // Convert record to source values and add to accumulator
                    let source_values = record_to_source_values(record, &cfg);
                    for (field_name, source) in source_values {
                        accumulator.add_source_value(field_name, source);
                    }

                    tracing::trace!(
                        "Worker {}: Accumulated sources for entity {} ({} sources total)",
                        worker_id, entity_id, accumulator.source_count
                    );
                }

                // Check which accumulators should resolve
                let mut to_resolve = Vec::new();
                for (entity_id, accumulator) in &accumulators {
                    if accumulator.should_resolve(&cfg, current_time) {
                        to_resolve.push(entity_id.clone());
                    }
                }

                // Resolve and emit golden records
                for entity_id in to_resolve {
                    if let Some(accumulator) = accumulators.remove(&entity_id) {
                        let start_resolve = std::time::Instant::now();

                        // Resolve all fields
                        match resolver.resolve_fields(&accumulator.entity_id, accumulator.fields.clone(), None) {
                            Ok(resolutions) => {
                                // Create golden record
                                match resolver.create_resolved_entity(&accumulator.entity_id, resolutions.clone()) {
                                    Ok(resolved_entity) => {
                                        let streaming_re = StreamingResolvedEntity {
                                            entity_id: resolved_entity.entity_id.clone(),
                                            fields: resolved_entity.fields,
                                            resolutions,
                                            overall_confidence: resolved_entity.overall_confidence,
                                            source_count: accumulator.source_count,
                                            conflict_count: resolved_entity.conflict_count,
                                            requires_review: resolved_entity.requires_review,
                                            updated_at: current_time,
                                        };

                                        let resolve_latency = start_resolve.elapsed();

                                        tracing::info!(
                                            "Worker {}: Created golden record for entity {} ({} fields, {:.2} confidence, {} sources, {:?} latency)",
                                            worker_id,
                                            streaming_re.entity_id,
                                            streaming_re.fields.len(),
                                            streaming_re.overall_confidence,
                                            streaming_re.source_count,
                                            resolve_latency
                                        );

                                        output_session.give(streaming_re);
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "Failed to create golden record for entity {}: {}",
                                            accumulator.entity_id, e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Failed to resolve fields for entity {}: {}",
                                    accumulator.entity_id, e
                                );
                            }
                        }
                    }
                }

                let total_latency = start.elapsed();
                if !buffer.is_empty() {
                    tracing::debug!(
                        "Worker {}: Processed batch in {:?} ({} active entities)",
                        worker_id, total_latency, accumulators.len()
                    );
                }
            });
        }
    })
}

/// Persist golden records to RDF store in real-time
///
/// Takes a stream of golden records and persists them using SPARQL UPDATE.
/// Uses FieldResolver to generate SPARQL INSERT queries and executes via the sink.
///
/// # Arguments
/// - `stream`: Stream of golden records to persist
/// - `sink`: RDF store sink implementing ResolvedEntitySink trait
/// - `voting_strategy`: Strategy type for FieldResolver (must match golden record creation)
///
/// # Returns
/// Stream of (entity_id, success) tuples for monitoring
pub fn persist_resolved_entities_to_rdf<G: Scope<Timestamp = u64>>(
    stream: Stream<G, StreamingResolvedEntity>,
    sink: Arc<dyn ResolvedEntitySink>,
    voting_strategy: StrategyType,
) -> Stream<G, (String, bool)> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "GoldenRecordRdfPersistence", move |_cap, info| {
        let mut buffer = Vec::new();
        let worker_id = info.global_id.to_string();
        let rdf_sink = sink.clone();

        // Create resolver for SPARQL generation (must match config from create_resolved_entities)
        let resolver = FieldResolver::with_strategy(voting_strategy);

        move |input, output| {
            input.for_each(|time, data| {
                let start = std::time::Instant::now();
                data.swap(&mut buffer);

                let mut output_session = output.session(&time);
                let mut success_count = 0;
                let mut failure_count = 0;

                for streaming_re in buffer.drain(..) {
                    let entity_id = streaming_re.entity_id.clone();

                    // Convert to GoldenRecord for SPARQL generation
                    let resolved_entity = streaming_re.to_resolved_entity();

                    // Generate SPARQL INSERT
                    let sparql = resolver.resolved_entity_to_sparql(&resolved_entity);

                    // Persist to RDF store
                    match rdf_sink.persist(&sparql) {
                        Ok(_) => {
                            tracing::debug!(
                                "Worker {}: Persisted golden record for entity {} ({} fields, {:.2} confidence)",
                                worker_id,
                                entity_id,
                                resolved_entity.fields.len(),
                                resolved_entity.overall_confidence
                            );
                            output_session.give((entity_id, true));
                            success_count += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Worker {}: Failed to persist golden record for entity {}: {}",
                                worker_id,
                                entity_id,
                                e
                            );
                            output_session.give((entity_id, false));
                            failure_count += 1;
                        }
                    }
                }

                let latency = start.elapsed();
                if success_count + failure_count > 0 {
                    tracing::info!(
                        "Worker {}: Persisted {} golden records ({} success, {} failures) in {:?}",
                        worker_id,
                        success_count + failure_count,
                        success_count,
                        failure_count,
                        latency
                    );
                }
            });
        }
    })
}

/// Cache golden records in real-time
///
/// Takes a stream of golden records and caches them for fast API access.
/// Designed for in-memory caches (e.g., DashMap-based) with TTL and eviction.
///
/// # Arguments
/// - `stream`: Stream of golden records to cache
/// - `cache`: Cache implementation (e.g., ResolvedEntityCache from coordinator)
///
/// # Returns
/// Stream of (entity_id, success) tuples for monitoring
pub fn cache_resolved_entities<G: Scope<Timestamp = u64>>(
    stream: Stream<G, StreamingResolvedEntity>,
    cache: Arc<dyn ResolvedEntityCache>,
) -> Stream<G, (String, bool)> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "GoldenRecordCaching", move |_cap, info| {
        let mut buffer = Vec::new();
        let worker_id = info.global_id.to_string();
        let cache_ref = cache.clone();

        move |input, output| {
            input.for_each(|time, data| {
                let start = std::time::Instant::now();
                data.swap(&mut buffer);

                let mut output_session = output.session(&time);
                let mut success_count = 0;
                let mut failure_count = 0;

                for streaming_re in buffer.drain(..) {
                    let entity_id = streaming_re.entity_id.clone();

                    // Insert into cache
                    match cache_ref.insert(streaming_re) {
                        Ok(_) => {
                            tracing::trace!(
                                "Worker {}: Cached golden record for entity {}",
                                worker_id,
                                entity_id
                            );
                            output_session.give((entity_id, true));
                            success_count += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                "Worker {}: Failed to cache golden record for entity {}: {}",
                                worker_id,
                                entity_id,
                                e
                            );
                            output_session.give((entity_id, false));
                            failure_count += 1;
                        }
                    }
                }

                let latency = start.elapsed();
                if success_count + failure_count > 0 {
                    tracing::debug!(
                        "Worker {}: Cached {} golden records ({} success, {} failures) in {:?}",
                        worker_id,
                        success_count + failure_count,
                        success_count,
                        failure_count,
                        latency
                    );
                }
            });
        }
    })
}

/// Conflict with severity classification
#[derive(Debug, Clone)]
pub struct ConflictEvent {
    /// Entity ID
    pub entity_id: String,

    /// Field name with conflict
    pub field_name: String,

    /// Conflict reason/description
    pub reason: String,

    /// Severity level
    pub severity: ConflictSeverity,

    /// Confidence of the selected value despite conflict
    pub confidence: Option<f64>,

    /// Number of conflicting source values
    pub conflicting_sources: usize,
}

/// Detect conflicts in real-time from golden records with severity classification
///
/// Analyzes field resolutions and extracts conflicts from resolutions.
/// Uses the severity classification already computed during field resolution.
///
/// # Returns
/// Stream of ConflictEvent with severity classification
pub fn detect_entity_conflicts<G: Scope<Timestamp = u64>>(
    stream: Stream<G, StreamingResolvedEntity>,
) -> Stream<G, ConflictEvent> {
    stream.flat_map(|gr| {
        gr.resolutions
            .into_iter()
            .filter_map(|resolution| {
                resolution.conflict.map(|conflict| ConflictEvent {
                    entity_id: gr.entity_id.clone(),
                    field_name: resolution.field_name,
                    reason: conflict.reason,
                    severity: conflict.severity,
                    confidence: resolution.selected_value.confidence,
                    conflicting_sources: conflict.conflicting_values.len(),
                })
            })
            .collect::<Vec<_>>()
    })
}

/// Aggregate conflicts by severity in real-time
///
/// Groups conflicts by severity level and counts occurrences.
/// Useful for monitoring and alerting.
///
/// # Returns
/// Stream of (severity, count) tuples
pub fn aggregate_entity_conflicts_by_severity<G: Scope<Timestamp = u64>>(
    stream: Stream<G, ConflictEvent>,
) -> Stream<G, (ConflictSeverity, usize)> {
    use timely::dataflow::channels::pact::Pipeline;
    use timely::dataflow::operators::generic::operator::Operator;

    stream.unary(Pipeline, "ConflictAggregation", move |_cap, _info| {
        let mut buffer = Vec::new();

        move |input, output| {
            input.for_each(|time, data| {
                data.swap(&mut buffer);

                // Count by severity (manually since ConflictSeverity doesn't implement Hash)
                let mut low = 0;
                let mut medium = 0;
                let mut high = 0;
                let mut critical = 0;

                for conflict in buffer.drain(..) {
                    match conflict.severity {
                        ConflictSeverity::Low => low += 1,
                        ConflictSeverity::Medium => medium += 1,
                        ConflictSeverity::High => high += 1,
                        ConflictSeverity::Critical => critical += 1,
                    }
                }

                // Emit counts
                let mut output_session = output.session(&time);
                if low > 0 {
                    output_session.give((ConflictSeverity::Low, low));
                }
                if medium > 0 {
                    output_session.give((ConflictSeverity::Medium, medium));
                }
                if high > 0 {
                    output_session.give((ConflictSeverity::High, high));
                }
                if critical > 0 {
                    output_session.give((ConflictSeverity::Critical, critical));
                }
            });
        }
    })
}

/// Filter critical conflicts requiring immediate review
///
/// Filters conflict stream to only include high and critical severity conflicts.
/// These should be routed to human reviewers or alerting systems.
pub fn filter_critical_entity_conflicts<G: Scope<Timestamp = u64>>(
    stream: Stream<G, ConflictEvent>,
) -> Stream<G, ConflictEvent> {
    stream.filter(|conflict| {
        matches!(
            conflict.severity,
            ConflictSeverity::High | ConflictSeverity::Critical
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use timely::Config;

    #[test]
    fn test_resolved_entity_operator() {
        use crate::core::lineage::{DataRef, LineageEvent};
        use std::sync::Arc;

        let config = Arc::new(ResolvedEntityConfig::default());

        timely::execute(Config::thread(), move |worker| {
            let cfg = config.clone();

            worker.dataflow::<u64, _, _>(move |scope| {
                use timely::dataflow::operators::Input;

                let (mut input, stream) = scope.new_input::<ProcessedRecord>();
                let _output = create_resolved_entities(stream, cfg.clone());

                // Send test record
                let test_record = ProcessedRecord {
                    record: Record {
                        id: "rec_001".to_string(),
                        dataset: "test_dataset".to_string(),
                        data: serde_json::json!({
                            "id": "entity_001",
                            "email": "test@example.com",
                            "name": "Test User"
                        }),
                        source: DataRef {
                            system: "test_dataset".to_string(),
                            path: "kafka://topic".to_string(),
                            version: None,
                            extracted_at: Utc::now(),
                            cdc_position: None,
                        },
                        timestamp: chrono::Utc::now().timestamp(),
                        tenant_id: "tenant1".to_string(),
                        semantic_metadata: None,
                    },
                    lineage: LineageEvent {
                        id: uuid::Uuid::new_v4(),
                        dataset: "test".to_string(),
                        record_id: "rec_001".to_string(),
                        source_refs: vec![],
                        transforms: vec![],
                        model_refs: vec![],
                        output_ref: DataRef {
                            system: "test".to_string(),
                            path: "test".to_string(),
                            version: None,
                            extracted_at: Utc::now(),
                            cdc_position: None,
                        },
                        ts: Utc::now(),
                        run_id: "test_run".to_string(),
                        tenant_id: "tenant1".to_string(),
                        correlation_id: None,
                        metadata: HashMap::new(),
                    },
                    violations: vec![],
                    quality_score: 1.0,
                };

                input.send(test_record);
                input.advance_to(1);
            });
        })
        .expect("Dataflow execution failed");
    }
}
