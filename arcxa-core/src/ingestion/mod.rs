//! # Ingestion Module
//!
//! CDC and batch data ingestion using Timely Dataflow and Differential Dataflow.
//! Handles Kafka streams, deduplication, standardization, and lineage capture.

pub mod dataflow;
pub mod dlq;
pub mod dlq_tiered;
// pub mod governance_bridge; // Moved to coordinator (uses storage)
pub mod kafka;
pub mod metrics; // Stub implementation (no-op), real metrics in coordinator
pub mod resolved_entities;
pub mod standardize; // Phase 2: Streaming resolved entity creation

// Re-export governance_bridge types for external use
// pub use governance_bridge::{GovernanceBridge, create_governance_bridge};

use crate::core::lineage::{DataRef, LineageEvent, TransformRef};
use crate::core::quality::QualityViolation;
use crate::inference::types::SemanticType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use timely::dataflow::*;
use uuid::Uuid;

/// Field-level semantic metadata for enriched records
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FieldSemanticMetadata {
    pub field_name: String,
    pub semantic_type: SemanticType,
    pub confidence: f64,
    pub detection_method: String,
}

/// Ingested record with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub dataset: String,
    #[serde(skip)] // Skip JSON value for Eq/Hash - use id for dedup
    pub data: serde_json::Value,
    pub source: DataRef,
    pub timestamp: i64,
    pub tenant_id: String,

    /// Phase 2: Semantic enrichment metadata (field-level)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_metadata: Option<HashMap<String, FieldSemanticMetadata>>,
}

// Manual PartialEq implementation (exclude semantic_metadata for dedup)
impl PartialEq for Record {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.dataset == other.dataset
            && self.source == other.source
            && self.timestamp == other.timestamp
            && self.tenant_id == other.tenant_id
    }
}

impl Eq for Record {}

impl std::hash::Hash for Record {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.dataset.hash(state);
        self.timestamp.hash(state);
        self.tenant_id.hash(state);
    }
}

impl PartialOrd for Record {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Record {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Order by timestamp, then id for deterministic ordering
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| self.id.cmp(&other.id))
    }
}

/// Processed record with quality results
#[derive(Debug, Clone)]
pub struct ProcessedRecord {
    pub record: Record,
    pub lineage: LineageEvent,
    pub violations: Vec<QualityViolation>,
    pub quality_score: f64,
}

/// Build the core Graphica dataflow pipeline
///
/// # Arguments
/// - `scope`: Timely dataflow scope
/// - `dedup_state`: Checkpointable dedup state (shared across workers)
/// - `semantic_detector`: Optional semantic type detector for Phase 2 enrichment
/// - `rdf_store`: Optional RDF triple store for Phase 2.1 persistence
/// - `golden_record_config`: Optional config for Phase 2 streaming golden records
///
/// # Architecture
/// Uses CheckpointableDedupState which is:
/// - Shared across all Timely workers (Arc/DashMap)
/// - Checkpointed periodically for exactly-once semantics
/// - Restored from checkpoints on restart
///
/// This ensures deduplication state survives restarts and provides cross-worker coordination.
///
/// **Phase 2: Semantic Enrichment**
/// - If semantic_detector is provided, enriches records with semantic type metadata
/// - Analyzes JSON field names using ColumnNameDetector
/// - Attaches FieldSemanticMetadata for downstream RDF conversion
///
/// **Phase 2.1: RDF Triple Persistence**
/// - If rdf_store is provided, persists semantic metadata as RDF triples
/// - Creates record entities, field entities, and detection provenance
/// - Writes to Turtle (.ttl) files organized by dataset
///
/// **Phase 2.2: Streaming Resolved Entities**
/// - If resolved_entity_config is provided, creates resolved entities in real-time
/// - Groups records by entity ID and accumulates source values
/// - Applies voting strategies for field resolution
/// - Emits StreamingResolvedEntity for downstream persistence
///
/// Note: Lineage events are captured and returned in ProcessedRecord. Integration with
/// storage (governance brain, RDF persistence) should be handled by the coordinator.
pub fn build_graphica_flow<G: Scope<Timestamp = u64>>(
    scope: &mut G,
    dedup_state: crate::checkpointing::CheckpointableDedupState,
    semantic_detector: Option<std::sync::Arc<crate::inference::semantic::ColumnNameDetector>>,
    rdf_store: Option<std::sync::Arc<crate::inference::rdf_store::RdfStore>>,
    resolved_entity_config: Option<std::sync::Arc<resolved_entities::ResolvedEntityConfig>>,
) -> (
    timely::dataflow::InputHandle<u64, Record>,
    Stream<G, ProcessedRecord>,
    Option<Stream<G, resolved_entities::StreamingResolvedEntity>>,
) {
    use timely::dataflow::operators::*;

    let (input_handle, input_stream) = scope.new_input::<Record>();

    // Step 1: Standardize and normalize data
    let standardized = input_stream
        .map(|rec| standardize::normalize_record(rec))
        .inspect(|rec| {
            tracing::debug!("Standardized record: {}", rec.id);
        });

    // Step 2: Deduplicate using checkpointable state
    // Integrates with checkpoint/recovery for exactly-once semantics
    let deduped = dataflow::deduplicate_with_state(standardized, dedup_state);

    // Step 2.5: PHASE 2 - Semantic enrichment (optional)
    // Analyzes JSON fields and infers semantic types from field names
    let enriched = if let Some(detector) = semantic_detector {
        dataflow::enrich_with_semantics(deduped, detector)
    } else {
        deduped
    };

    // Step 2.6: PHASE 2.1 - RDF triple persistence (optional)
    // Persists semantic metadata as RDF triples to file-based store
    let with_rdf = if let Some(store) = rdf_store {
        dataflow::persist_rdf_semantics(enriched, store)
    } else {
        enriched
    };

    // Step 3: Capture lineage at each transformation
    let with_lineage = with_rdf.map(|rec| {
        let lineage = dataflow::capture_lineage(&rec);
        (rec, lineage)
    });

    // Step 4: Apply quality rules
    let with_quality = with_lineage.map(|(rec, lineage)| {
        let violations = dataflow::apply_quality_rules(&rec);
        let quality_score = dataflow::calculate_quality_score(&violations);

        ProcessedRecord {
            record: rec,
            lineage,
            violations,
            quality_score,
        }
    });

    // Step 5: Profile and collect statistics
    let profiled = with_quality.inspect(|processed| {
        dataflow::update_profile_statistics(&processed.record);
    });

    // Step 6: PHASE 2.2 - Streaming resolved entities (optional)
    // Creates resolved entities in real-time from CDC stream
    let resolved_entities = if let Some(config) = resolved_entity_config {
        Some(resolved_entities::create_resolved_entities(
            profiled.clone(),
            config,
        ))
    } else {
        None
    };

    (input_handle, profiled, resolved_entities)
}

#[cfg(test)]
mod tests {
    use super::*;
    use timely::dataflow::operators::Inspect;
    use timely::Config;

    #[test]
    fn test_dataflow_construction() {
        use std::sync::Arc;

        let dedup_state = Arc::new(crate::checkpointing::CheckpointableDedupState::new(
            60_000, 100_000,
        ));

        timely::execute(Config::thread(), move |worker| {
            let dedup = (*dedup_state).clone();
            worker.dataflow::<u64, _, _>(move |scope| {
                let (_input, _output, _resolved_entities) =
                    build_graphica_flow(scope, dedup.clone(), None, None, None);
            });
        })
        .expect("Dataflow execution failed");
    }

    #[test]
    fn test_dataflow_with_semantic_enrichment() {
        use crate::inference::semantic::ColumnNameDetector;
        use std::sync::Arc;

        let dedup_state = Arc::new(crate::checkpointing::CheckpointableDedupState::new(
            60_000, 100_000,
        ));
        let detector = Arc::new(ColumnNameDetector::new());

        timely::execute(Config::thread(), move |worker| {
            let dedup = (*dedup_state).clone();
            let det = detector.clone();
            worker.dataflow::<u64, _, _>(move |scope| {
                let (_input, _output, _resolved_entities) =
                    build_graphica_flow(scope, dedup.clone(), Some(det.clone()), None, None);
            });
        })
        .expect("Dataflow execution failed");
    }

    #[test]
    fn test_end_to_end_semantic_enrichment() {
        use crate::core::lineage::DataRef;
        use crate::inference::semantic::ColumnNameDetector;
        use crate::inference::types::SemanticType;
        use chrono::Utc;
        use serde_json::json;
        use std::sync::{Arc, Mutex};

        let dedup_state = Arc::new(crate::checkpointing::CheckpointableDedupState::new(
            60_000, 100_000,
        ));
        let detector = Arc::new(ColumnNameDetector::new());
        let results = Arc::new(Mutex::new(Vec::new()));

        let results_clone = results.clone();

        timely::execute(Config::thread(), move |worker| {
            let dedup = (*dedup_state).clone();
            let det = detector.clone();
            let res = results_clone.clone();

            worker.dataflow::<u64, _, _>(move |scope| {
                let (mut input, output, _resolved_entities) =
                    build_graphica_flow(scope, dedup.clone(), Some(det.clone()), None, None);

                // Inject test record with semantic fields
                let test_record = Record {
                    id: "test-001".to_string(),
                    dataset: "customers".to_string(),
                    data: json!({
                        "email": "john@example.com",
                        "phone_number": "555-1234",
                        "full_name": "John Doe",
                        "billing_address": "123 Main St",
                        "city": "San Francisco",
                        "zip_code": "94102",
                        "timestamp": "2024-01-15T10:30:00Z",
                        "unknown_field": "some_value"
                    }),
                    source: DataRef {
                        system: "test-system".to_string(),
                        path: "customers".to_string(),
                        version: Some("1.0".to_string()),
                        extracted_at: Utc::now(),
                        cdc_position: None,
                    },
                    timestamp: 1234567890,
                    tenant_id: "test-tenant".to_string(),
                    semantic_metadata: None,
                };

                input.send(test_record);
                input.advance_to(1);

                // Collect results
                let res_inner = res.clone();
                output.inspect(move |processed| {
                    res_inner.lock().unwrap().push(processed.clone());
                });
            });
        })
        .expect("Dataflow execution failed");

        // Verify results
        let results = results.lock().unwrap();
        assert_eq!(results.len(), 1, "Should process exactly one record");

        let processed = &results[0];
        let metadata = processed
            .record
            .semantic_metadata
            .as_ref()
            .expect("Record should have semantic metadata");

        // Verify detected semantic types
        assert!(metadata.contains_key("email"));
        assert_eq!(metadata["email"].semantic_type, SemanticType::Email);
        assert!(metadata["email"].confidence > 0.8);

        assert!(metadata.contains_key("phone_number"));
        assert_eq!(
            metadata["phone_number"].semantic_type,
            SemanticType::PhoneNumber
        );

        assert!(metadata.contains_key("full_name"));
        assert_eq!(
            metadata["full_name"].semantic_type,
            SemanticType::PersonName
        );

        assert!(metadata.contains_key("billing_address"));
        assert_eq!(
            metadata["billing_address"].semantic_type,
            SemanticType::Address
        );

        assert!(metadata.contains_key("city"));
        assert_eq!(metadata["city"].semantic_type, SemanticType::City);

        assert!(metadata.contains_key("zip_code"));
        assert_eq!(metadata["zip_code"].semantic_type, SemanticType::PostalCode);

        assert!(metadata.contains_key("timestamp"));
        assert_eq!(metadata["timestamp"].semantic_type, SemanticType::Timestamp);

        // Unknown field should not be detected
        assert!(!metadata.contains_key("unknown_field"));

        println!(
            "✓ Semantic enrichment correctly detected {} field types",
            metadata.len()
        );
    }
}
