//! # Lineage Module
//!
//! Captures complete data provenance from source → transforms → models → outputs.
//! Supports multi-year queries and traceability for regulatory compliance.

pub mod async_sink;
pub mod column_level;
pub mod impact;
pub mod row_level;
pub mod schema_evolution;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub use async_sink::{AsyncLineageSink, SyncToAsyncAdapter};
pub use impact::{
    ChangeType, ImpactAnalyzer, ImpactReport, ProposedChange, RiskLevel, RootCauseReport,
    SimulationReport,
};

/// Complete lineage event capturing all provenance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEvent {
    /// Unique lineage event ID
    pub id: Uuid,
    /// Dataset identifier
    pub dataset: String,
    /// Record identifier within dataset
    pub record_id: String,
    /// Original data sources
    pub source_refs: Vec<DataRef>,
    /// Transformation steps applied
    pub transforms: Vec<TransformRef>,
    /// ML/AI models that processed this record
    pub model_refs: Vec<ModelRef>,
    /// Final integrated output location
    pub output_ref: DataRef,
    /// Event timestamp
    pub ts: DateTime<Utc>,
    /// Pipeline run identifier
    pub run_id: String,
    /// Tenant/organization identifier
    pub tenant_id: String,
    /// Correlation ID for distributed tracing
    pub correlation_id: Option<String>,
    /// Optional metadata
    pub metadata: HashMap<String, String>,
}

/// Reference to a data source or sink
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DataRef {
    /// Source system (e.g., "salesforce", "postgres-prod")
    pub system: String,
    /// Resource path (e.g., "accounts/123", "public.customers")
    pub path: String,
    /// Version/snapshot identifier
    pub version: Option<String>,
    /// Timestamp of data extraction
    pub extracted_at: DateTime<Utc>,
    /// CDC log position (for streaming sources)
    pub cdc_position: Option<CdcPosition>,
}

/// CDC position tracking for replay and ordering
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CdcPosition {
    /// Topic name
    pub topic: String,
    /// Partition
    pub partition: i32,
    /// Offset
    pub offset: i64,
    /// LSN or equivalent for databases
    pub lsn: Option<String>,
}

/// Transformation applied to data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformRef {
    /// Transform ID
    pub id: Uuid,
    /// Transform type (e.g., "dedupe", "standardize", "enrich")
    pub transform_type: String,
    /// Rule or script identifier
    pub rule_id: String,
    /// Version of the transformation logic
    pub version: String,
    /// Parameters used
    pub parameters: HashMap<String, serde_json::Value>,
    /// Applied timestamp
    pub applied_at: DateTime<Utc>,
    /// Fields affected
    pub fields_modified: Vec<String>,
}

/// ML/AI model reference with full metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Model unique identifier
    pub model_id: String,
    /// Model version (semantic versioning)
    pub version: String,
    /// Model type (e.g., "sklearn.RandomForest", "pytorch.Transformer")
    pub model_type: String,
    /// Hash of model parameters/weights for reproducibility
    pub params_hash: String,
    /// Training dataset references
    pub training_data: Vec<DataRef>,
    /// Validation metrics
    pub metrics: ModelMetrics,
    /// Model registry URI
    pub registry_uri: String,
    /// Inference timestamp
    pub inference_at: DateTime<Utc>,
    /// Features used
    pub features_used: Vec<String>,
    /// Predictions/outputs generated
    pub outputs: Vec<String>,
}

/// Model performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub accuracy: Option<f64>,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f1_score: Option<f64>,
    pub rmse: Option<f64>,
    pub custom_metrics: HashMap<String, f64>,
}

/// Trait for storing and querying lineage
pub trait LineageSink: Send + Sync {
    /// Persist lineage event
    fn write(&self, event: LineageEvent) -> anyhow::Result<()>;

    /// Query lineage for a specific record
    fn get_record_lineage(&self, record_id: &str) -> anyhow::Result<Vec<LineageEvent>>;

    /// Query all data affected by a model
    fn get_model_impact(&self, model_id: &str, version: &str) -> anyhow::Result<Vec<LineageEvent>>;

    /// Query lineage by time range
    fn query_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> anyhow::Result<Vec<LineageEvent>>;

    /// Get lineage for a specific run
    fn get_run_lineage(&self, run_id: &str) -> anyhow::Result<Vec<LineageEvent>>;

    /// Time-travel query: Get lineage as it existed at a specific timestamp
    /// KEY DIFFERENTIATOR: Enables "show me the lineage 6 months ago when this model was trained"
    fn get_lineage_as_of(
        &self,
        record_id: &str,
        as_of: DateTime<Utc>,
    ) -> anyhow::Result<Vec<LineageEvent>>;
}

/// Lineage graph for impact analysis with recursive traversal
#[derive(Debug, Clone)]
pub struct LineageGraph {
    events: Vec<LineageEvent>,
    /// Index: record_id -> event indices
    record_index: hashbrown::HashMap<String, Vec<usize>>,
    /// Index: dataset+path -> record_ids
    data_ref_index: hashbrown::HashMap<String, hashbrown::HashSet<String>>,
}

impl LineageGraph {
    pub fn new(events: Vec<LineageEvent>) -> Self {
        let mut record_index: hashbrown::HashMap<String, Vec<usize>> = hashbrown::HashMap::new();
        let mut data_ref_index: hashbrown::HashMap<String, hashbrown::HashSet<String>> =
            hashbrown::HashMap::new();

        for (idx, event) in events.iter().enumerate() {
            // Build record index
            record_index
                .entry(event.record_id.clone())
                .or_insert_with(Vec::new)
                .push(idx);

            // Build data ref index (output_ref points to this record)
            let output_key = format!("{}:{}", event.output_ref.system, event.output_ref.path);
            data_ref_index
                .entry(output_key)
                .or_insert_with(hashbrown::HashSet::new)
                .insert(event.record_id.clone());
        }

        Self {
            events,
            record_index,
            data_ref_index,
        }
    }

    /// Alias for new() - for consistency with impact analyzer API
    pub fn from_events(events: Vec<LineageEvent>) -> Self {
        Self::new(events)
    }

    /// Get reference to all events in the graph
    pub fn events(&self) -> &[LineageEvent] {
        &self.events
    }

    /// Get all events for a record
    fn get_events_for_record(&self, record_id: &str) -> Vec<&LineageEvent> {
        if let Some(indices) = self.record_index.get(record_id) {
            indices.iter().map(|&idx| &self.events[idx]).collect()
        } else {
            vec![]
        }
    }

    /// Compute immediate upstream dependencies (direct sources)
    pub fn upstream(&self, record_id: &str) -> Vec<DataRef> {
        self.get_events_for_record(record_id)
            .into_iter()
            .flat_map(|e| e.source_refs.iter().cloned())
            .collect()
    }

    /// Compute ALL upstream dependencies recursively (transitive closure)
    pub fn upstream_recursive(
        &self,
        record_id: &str,
        max_depth: usize,
    ) -> hashbrown::HashSet<DataRef> {
        let mut visited = hashbrown::HashSet::new();
        let mut to_visit = vec![(record_id.to_string(), 0)];
        let mut result = hashbrown::HashSet::new();

        while let Some((current_id, depth)) = to_visit.pop() {
            if depth > max_depth || visited.contains(&current_id) {
                continue;
            }

            visited.insert(current_id.clone());

            for event in self.get_events_for_record(&current_id) {
                for source_ref in &event.source_refs {
                    result.insert(source_ref.clone());

                    // Try to find records that produced this source
                    let source_key = format!("{}:{}", source_ref.system, source_ref.path);
                    if let Some(source_records) = self.data_ref_index.get(&source_key) {
                        for source_record_id in source_records {
                            if !visited.contains(source_record_id) {
                                to_visit.push((source_record_id.clone(), depth + 1));
                            }
                        }
                    }
                }
            }
        }

        result
    }

    /// Compute immediate downstream consumers (direct outputs)
    pub fn downstream(&self, record_id: &str) -> Vec<DataRef> {
        self.get_events_for_record(record_id)
            .into_iter()
            .map(|e| e.output_ref.clone())
            .collect()
    }

    /// Compute ALL downstream consumers recursively
    pub fn downstream_recursive(
        &self,
        record_id: &str,
        max_depth: usize,
    ) -> hashbrown::HashSet<String> {
        let mut visited = hashbrown::HashSet::new();
        let mut to_visit = vec![(record_id.to_string(), 0)];
        let mut result = hashbrown::HashSet::new();

        while let Some((current_id, depth)) = to_visit.pop() {
            if depth > max_depth || visited.contains(&current_id) {
                continue;
            }

            visited.insert(current_id.clone());

            for event in self.get_events_for_record(&current_id) {
                let output_key = format!("{}:{}", event.output_ref.system, event.output_ref.path);

                // Find records that consume this output
                if let Some(consumer_records) = self.data_ref_index.get(&output_key) {
                    for consumer_id in consumer_records {
                        if !visited.contains(consumer_id) {
                            result.insert(consumer_id.clone());
                            to_visit.push((consumer_id.clone(), depth + 1));
                        }
                    }
                }
            }
        }

        result
    }

    /// Extract all models in lineage chain
    pub fn models_in_chain(&self, record_id: &str) -> Vec<ModelRef> {
        self.get_events_for_record(record_id)
            .into_iter()
            .flat_map(|e| e.model_refs.iter().cloned())
            .collect()
    }

    /// Get full lineage chain (all upstream and downstream) as a subgraph
    pub fn full_lineage_chain(&self, record_id: &str, max_depth: usize) -> LineageGraph {
        let mut record_ids = hashbrown::HashSet::new();
        record_ids.insert(record_id.to_string());

        // Get all upstream
        let upstream = self.upstream_recursive(record_id, max_depth);
        for data_ref in upstream {
            let key = format!("{}:{}", data_ref.system, data_ref.path);
            if let Some(records) = self.data_ref_index.get(&key) {
                record_ids.extend(records.iter().cloned());
            }
        }

        // Get all downstream
        let downstream = self.downstream_recursive(record_id, max_depth);
        record_ids.extend(downstream);

        // Collect all events for these records
        let mut chain_events = Vec::new();
        for rid in record_ids {
            chain_events.extend(self.get_events_for_record(&rid).into_iter().cloned());
        }

        LineageGraph::new(chain_events)
    }

    /// Detect circular dependencies
    pub fn has_circular_dependency(&self, record_id: &str) -> bool {
        let mut visited = hashbrown::HashSet::new();
        let mut recursion_stack = hashbrown::HashSet::new();

        self.detect_cycle(record_id, &mut visited, &mut recursion_stack)
    }

    fn detect_cycle(
        &self,
        record_id: &str,
        visited: &mut hashbrown::HashSet<String>,
        recursion_stack: &mut hashbrown::HashSet<String>,
    ) -> bool {
        visited.insert(record_id.to_string());
        recursion_stack.insert(record_id.to_string());

        for event in self.get_events_for_record(record_id) {
            let output_key = format!("{}:{}", event.output_ref.system, event.output_ref.path);

            if let Some(consumers) = self.data_ref_index.get(&output_key) {
                for consumer_id in consumers {
                    if !visited.contains(consumer_id) {
                        if self.detect_cycle(consumer_id, visited, recursion_stack) {
                            return true;
                        }
                    } else if recursion_stack.contains(consumer_id) {
                        return true; // Cycle detected
                    }
                }
            }
        }

        recursion_stack.remove(record_id);
        false
    }

    /// Calculate lineage depth (longest path from sources)
    pub fn lineage_depth(&self, record_id: &str) -> usize {
        let mut max_depth = 0;
        let mut visited = hashbrown::HashSet::new();

        fn dfs(
            graph: &LineageGraph,
            record_id: &str,
            depth: usize,
            visited: &mut hashbrown::HashSet<String>,
        ) -> usize {
            if visited.contains(record_id) {
                return depth;
            }

            visited.insert(record_id.to_string());

            let mut max_child_depth = depth;

            for event in graph.get_events_for_record(record_id) {
                for source_ref in &event.source_refs {
                    let source_key = format!("{}:{}", source_ref.system, source_ref.path);
                    if let Some(source_records) = graph.data_ref_index.get(&source_key) {
                        for source_id in source_records {
                            let child_depth = dfs(graph, source_id, depth + 1, visited);
                            max_child_depth = max_child_depth.max(child_depth);
                        }
                    }
                }
            }

            max_child_depth
        }

        dfs(self, record_id, 0, &mut visited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lineage_event_creation() {
        let event = LineageEvent {
            id: Uuid::new_v4(),
            dataset: "customers".to_string(),
            record_id: "cust-12345".to_string(),
            source_refs: vec![],
            transforms: vec![],
            model_refs: vec![],
            output_ref: DataRef {
                system: "warehouse".to_string(),
                path: "dim_customer/123".to_string(),
                version: Some("v1".to_string()),
                extracted_at: Utc::now(),
                cdc_position: None,
            },
            ts: Utc::now(),
            run_id: "run-001".to_string(),
            tenant_id: "tenant-a".to_string(),
            correlation_id: None,
            metadata: HashMap::new(),
        };

        assert_eq!(event.dataset, "customers");
        assert_eq!(event.record_id, "cust-12345");
    }
}
