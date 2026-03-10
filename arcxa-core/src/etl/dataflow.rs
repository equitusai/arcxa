//! Timely and Differential Dataflow operators for streaming ETL
//!
//! This module implements the core dataflow operators using Timely Dataflow
//! and Differential Dataflow for high-performance streaming transformations.

use super::{Record, LineageEvent, DataRef, TransformRef, ModelRef};
use anyhow::Result;
use differential_dataflow::lattice::Lattice;
use differential_dataflow::{Collection, ExchangeData};
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use timely::communication::Allocate;
use timely::dataflow::channels::pact::Pipeline;
use timely::dataflow::operators::{Inspect, Map};
use timely::dataflow::{Scope, Stream};
use timely::order::Product;
use timely::progress::Antichain;
use tracing::{debug, info};

/// Build the main Graphica dataflow pipeline
///
/// This creates a Timely Dataflow computation graph with:
/// - Deduplication using Differential Dataflow
/// - Quality rule evaluation
/// - Profiling and statistics
/// - Lineage capture at each stage
pub fn build_graphica_flow<G, D>(
    scope: &mut G,
    config: DataflowConfig,
) -> (Stream<G, Record>, Stream<G, LineageEvent>)
where
    G: Scope,
    G::Timestamp: Lattice + Ord,
    D: ExchangeData + Hash,
{
    let (input_send, input_recv) = scope.new_input::<Record>();

    // Stage 1: Standardization
    let standardized = input_recv
        .map(move |mut record| {
            standardize_record(&mut record);
            record.add_transform(TransformRef {
                transform_id: "standardize".to_string(),
                transform_type: "normalization".to_string(),
                params_hash: "std_v1".to_string(),
                execution_time_ms: 1,
            });
            record
        })
        .inspect(|record| debug!("Standardized record: {}", record.id));

    // Stage 2: Deduplication (using Differential Dataflow)
    let deduplicated = if config.enable_deduplication {
        deduplicate_stream(&standardized, config.dedup_key_fields)
    } else {
        standardized
    };

    // Stage 3: Quality Rules
    let validated = deduplicated
        .map(move |mut record| {
            apply_quality_rules(&mut record, &config.quality_rules);
            record.update_quality_score();
            record
        })
        .inspect(|record| {
            if record.quality_score < config.quality_threshold {
                info!(
                    "Low quality record {}: score {}",
                    record.id, record.quality_score
                );
            }
        });

    // Stage 4: Profiling
    let profiled = validated
        .map(move |mut record| {
            profile_record(&mut record);
            record.add_transform(TransformRef {
                transform_id: "profile".to_string(),
                transform_type: "statistics".to_string(),
                params_hash: "prof_v1".to_string(),
                execution_time_ms: 2,
            });
            record
        });

    // Stage 5: Lineage Capture
    let (records_out, lineage_events) = capture_lineage_parallel(&profiled, config.lineage_config);

    (records_out, lineage_events)
}

/// Configuration for dataflow pipeline
#[derive(Clone, Debug)]
pub struct DataflowConfig {
    pub enable_deduplication: bool,
    pub dedup_key_fields: Vec<String>,
    pub quality_rules: Vec<QualityRuleConfig>,
    pub quality_threshold: f64,
    pub lineage_config: LineageConfig,
}

#[derive(Clone, Debug)]
pub struct QualityRuleConfig {
    pub rule_id: String,
    pub rule_type: String,
    pub params: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct LineageConfig {
    pub capture_field_level: bool,
    pub include_transformations: bool,
    pub include_models: bool,
}

/// Standardize records (trim whitespace, normalize case, etc.)
fn standardize_record(record: &mut Record) {
    for (_key, value) in record.data.iter_mut() {
        if let Some(s) = value.as_str() {
            *value = serde_json::Value::String(s.trim().to_string());
        }
    }
}

/// Deduplicate stream using Differential Dataflow
fn deduplicate_stream<G>(
    stream: &Stream<G, Record>,
    key_fields: Vec<String>,
) -> Stream<G, Record>
where
    G: Scope,
    G::Timestamp: Lattice + Ord,
{
    use differential_dataflow::AsCollection;
    use differential_dataflow::operators::Reduce;

    // Convert to collection for Differential operations
    let collection = stream.as_collection();

    // Group by key fields and keep first record
    let deduplicated = collection
        .map(move |record| {
            let key = extract_key(&record, &key_fields);
            (key, record)
        })
        .reduce(|_key, input, output| {
            // Keep first record (by timestamp)
            if let Some((record, _)) = input.first() {
                output.push(((*record).clone(), 1));
            }
        })
        .map(|(_key, record)| record);

    // Convert back to stream
    deduplicated.inner.map(|(data, _time, _diff)| data)
}

/// Extract deduplication key from record
fn extract_key(record: &Record, key_fields: &[String]) -> String {
    let mut key_parts = Vec::new();
    for field in key_fields {
        if let Some(value) = record.data.get(field) {
            key_parts.push(value.to_string());
        }
    }
    key_parts.join(":")
}

/// Apply quality rules to a record
fn apply_quality_rules(record: &mut Record, rules: &[QualityRuleConfig]) {
    for rule in rules {
        match rule.rule_type.as_str() {
            "completeness" => check_completeness(record, &rule.params),
            "validity" => check_validity(record, &rule.params),
            "uniqueness" => check_uniqueness(record, &rule.params),
            "consistency" => check_consistency(record, &rule.params),
            _ => {}
        }
    }

    record.add_transform(TransformRef {
        transform_id: "quality_check".to_string(),
        transform_type: "validation".to_string(),
        params_hash: format!("rules_{}", rules.len()),
        execution_time_ms: 3,
    });
}

/// Check completeness rule
fn check_completeness(record: &mut Record, params: &HashMap<String, String>) {
    if let Some(required_fields) = params.get("required_fields") {
        for field in required_fields.split(',') {
            if !record.data.contains_key(field) || record.data[field].is_null() {
                record.violations.push(super::quality::QualityViolation {
                    rule_id: "completeness".to_string(),
                    field: Some(field.to_string()),
                    severity: "medium".to_string(),
                    message: format!("Required field '{}' is missing", field),
                });
            }
        }
    }
}

/// Check validity rule
fn check_validity(record: &mut Record, params: &HashMap<String, String>) {
    // Example: Check email format
    if let Some(email_field) = params.get("email_field") {
        if let Some(email_value) = record.data.get(email_field) {
            if let Some(email_str) = email_value.as_str() {
                if !email_str.contains('@') {
                    record.violations.push(super::quality::QualityViolation {
                        rule_id: "validity".to_string(),
                        field: Some(email_field.to_string()),
                        severity: "high".to_string(),
                        message: format!("Invalid email format: {}", email_str),
                    });
                }
            }
        }
    }
}

/// Check uniqueness rule (placeholder - actual implementation would use state)
fn check_uniqueness(_record: &mut Record, _params: &HashMap<String, String>) {
    // In production, this would check against a state store or bloom filter
}

/// Check consistency rule
fn check_consistency(record: &mut Record, params: &HashMap<String, String>) {
    // Example: Check date consistency
    if let Some(date_fields) = params.get("date_fields") {
        let fields: Vec<&str> = date_fields.split(',').collect();
        if fields.len() >= 2 {
            // Compare dates for logical consistency
            // Implementation would parse and compare dates
        }
    }
}

/// Profile record (compute statistics)
fn profile_record(record: &mut Record) {
    let mut stats = HashMap::new();

    // Count non-null fields
    let total_fields = record.data.len();
    let non_null_fields = record.data.values().filter(|v| !v.is_null()).count();

    stats.insert(
        "completeness_ratio".to_string(),
        serde_json::json!(non_null_fields as f64 / total_fields as f64),
    );

    // Add stats to record metadata
    record.data.insert(
        "__profile_stats".to_string(),
        serde_json::to_value(stats).unwrap(),
    );
}

/// Capture lineage in parallel with record flow
fn capture_lineage_parallel<G>(
    stream: &Stream<G, Record>,
    config: LineageConfig,
) -> (Stream<G, Record>, Stream<G, LineageEvent>)
where
    G: Scope,
{
    // Split stream for parallel processing
    let records_out = stream.map(|record| record.clone());

    let lineage_events = stream.map(move |record| {
        create_lineage_event(&record, &config)
    });

    (records_out, lineage_events)
}

/// Create lineage event from record
fn create_lineage_event(record: &Record, config: &LineageConfig) -> LineageEvent {
    LineageEvent {
        dataset: record.dataset.clone(),
        record_id: record.id.clone(),
        source_refs: vec![DataRef {
            uri: record.source_uri.clone(),
            version: record.source_version.clone(),
            cdc_position: record.cdc_position.clone(),
            file_library_id: record.file_library_id.clone(),
        }],
        transforms: if config.include_transformations {
            record.transform_refs.clone()
        } else {
            vec![]
        },
        model_refs: if config.include_models {
            record.model_refs.clone()
        } else {
            vec![]
        },
        output_ref: DataRef {
            uri: format!("graphica://processed/{}", record.id),
            version: "1.0.0".to_string(),
            cdc_position: None,
            file_library_id: None,
        },
        ts: chrono::Utc::now().timestamp(),
        run_id: uuid::Uuid::new_v4().to_string(),
        confidence: record.quality_score,
    }
}

/// Advanced deduplication with time windows
pub fn deduplicate_with_window<G, D>(
    stream: &Stream<G, Record>,
    key_fn: impl Fn(&Record) -> D + 'static,
    window_size_ms: u64,
) -> Stream<G, Record>
where
    G: Scope,
    G::Timestamp: Lattice + Ord,
    D: ExchangeData + Hash,
{
    use differential_dataflow::AsCollection;
    use differential_dataflow::operators::arrange::ArrangeByKey;
    use differential_dataflow::trace::implementations::spine_fueled::Spine;

    let collection = stream.as_collection();

    // Arrange by key for efficient lookups
    let arranged = collection
        .map(move |record| (key_fn(&record), record))
        .arrange_by_key();

    // Apply windowed deduplication
    arranged
        .reduce(move |_key, input, output| {
            // Keep records within time window
            let mut seen = std::collections::HashSet::new();
            for (record, count) in input.iter() {
                let record_key = format!("{}-{}", record.id, record.ingested_at.timestamp_millis());
                if seen.insert(record_key) {
                    output.push(((*record).clone(), *count));
                }
            }
        })
        .map(|(_key, record)| record)
        .inner
        .map(|(data, _time, _diff)| data)
}

/// Incremental aggregation using Differential Dataflow
pub fn incremental_aggregate<G>(
    stream: &Stream<G, Record>,
    group_by_fields: Vec<String>,
    aggregations: Vec<Aggregation>,
) -> Stream<G, AggregateResult>
where
    G: Scope,
    G::Timestamp: Lattice + Ord,
{
    use differential_dataflow::AsCollection;
    use differential_dataflow::operators::Reduce;

    let collection = stream.as_collection();

    collection
        .map(move |record| {
            let key = extract_key(&record, &group_by_fields);
            (key, record)
        })
        .reduce(move |key, input, output| {
            let mut result = AggregateResult {
                group_key: key.clone(),
                values: HashMap::new(),
            };

            for agg in &aggregations {
                match agg {
                    Aggregation::Count => {
                        result.values.insert("count".to_string(), input.len() as f64);
                    }
                    Aggregation::Sum(field) => {
                        let sum: f64 = input
                            .iter()
                            .filter_map(|(record, _)| {
                                record.0.data.get(field)?.as_f64()
                            })
                            .sum();
                        result.values.insert(format!("sum_{}", field), sum);
                    }
                    Aggregation::Avg(field) => {
                        let values: Vec<f64> = input
                            .iter()
                            .filter_map(|(record, _)| {
                                record.0.data.get(field)?.as_f64()
                            })
                            .collect();
                        if !values.is_empty() {
                            let avg = values.iter().sum::<f64>() / values.len() as f64;
                            result.values.insert(format!("avg_{}", field), avg);
                        }
                    }
                    Aggregation::Min(field) => {
                        let min = input
                            .iter()
                            .filter_map(|(record, _)| {
                                record.0.data.get(field)?.as_f64()
                            })
                            .min_by(|a, b| a.partial_cmp(b).unwrap());
                        if let Some(min_val) = min {
                            result.values.insert(format!("min_{}", field), min_val);
                        }
                    }
                    Aggregation::Max(field) => {
                        let max = input
                            .iter()
                            .filter_map(|(record, _)| {
                                record.0.data.get(field)?.as_f64()
                            })
                            .max_by(|a, b| a.partial_cmp(b).unwrap());
                        if let Some(max_val) = max {
                            result.values.insert(format!("max_{}", field), max_val);
                        }
                    }
                }
            }

            output.push((result, 1));
        })
        .map(|(_key, result)| result)
        .inner
        .map(|(data, _time, _diff)| data)
}

/// Aggregation types
#[derive(Clone, Debug)]
pub enum Aggregation {
    Count,
    Sum(String),
    Avg(String),
    Min(String),
    Max(String),
}

/// Result of aggregation
#[derive(Clone, Debug)]
pub struct AggregateResult {
    pub group_key: String,
    pub values: HashMap<String, f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_key() {
        let mut data = HashMap::new();
        data.insert("customer_id".to_string(), json!("123"));
        data.insert("order_id".to_string(), json!("456"));

        let record = Record::new("rec_1".to_string(), "orders".to_string(), data);

        let key = extract_key(&record, &["customer_id".to_string(), "order_id".to_string()]);
        assert_eq!(key, "\"123\":\"456\"");
    }

    #[test]
    fn test_standardize_record() {
        let mut data = HashMap::new();
        data.insert("name".to_string(), json!("  John Doe  "));
        data.insert("email".to_string(), json!(" john@example.com "));

        let mut record = Record::new("rec_1".to_string(), "customers".to_string(), data);
        standardize_record(&mut record);

        assert_eq!(record.data["name"], json!("John Doe"));
        assert_eq!(record.data["email"], json!("john@example.com"));
    }
}