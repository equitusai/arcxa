use super::*;
use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
use crate::orchestration::rules::RuleExecutor;
use crate::orchestration::workflow::definition::{
    ConfidenceGateConfig, FallbackStrategy, StepConfig, StepType, WorkflowDefinition, WorkflowStep,
};
use std::collections::HashMap;
use std::sync::Arc;

fn create_test_workflow() -> WorkflowDefinition {
    WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "gate1".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.5,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    }
}

#[test]
fn test_extract_materializable_rows_prefers_internal_rows() {
    let output = serde_json::json!({
        "_rows": [{"id": 1}, {"id": 2}],
        "rows": [{"id": 99}]
    });

    let rows = extract_materializable_rows(&output).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], 1);
}

#[test]
fn test_extract_materializable_rows_supports_rows_key() {
    let output = serde_json::json!({
        "rows": [{"name": "Alice"}]
    });

    let rows = extract_materializable_rows(&output).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["name"], "Alice");
}

#[test]
fn test_estimate_json_memory_counts_nested_json_shape() {
    let payload = serde_json::json!({
        "status": "ok",
        "rows": [
            {"id": 1},
            {"id": 2}
        ]
    });

    let estimated = WorkflowExecutor::estimate_json_memory(&payload);
    assert_eq!(estimated, 168);
}

#[test]
fn test_parse_row_id_key_supports_databricks() {
    let row_id = parse_row_id_key("databricks:main.bronze.events:event_id=evt-123").unwrap();
    assert_eq!(
        row_id.to_key(),
        "databricks:main.bronze.events:event_id=evt-123"
    );
}

#[test]
fn test_parse_row_id_key_supports_csv_and_database_fallback_index() {
    let csv = parse_row_id_key("csv:/tmp/events.csv:42").unwrap();
    assert_eq!(csv.to_key(), "csv:/tmp/events.csv:42");

    let db = parse_row_id_key("databricks:main.bronze.events:_row_index=1").unwrap();
    assert_eq!(db.to_key(), "databricks:main.bronze.events:_row_index=1");
}

#[test]
fn test_build_rows_output_preserves_rows_count_and_extra_fields() {
    let output = build_rows_output(
        vec![
            serde_json::json!({"id": 1, "name": "Alice"}),
            serde_json::json!({"id": 2, "name": "Bob"}),
        ],
        2,
        vec![
            ("status".to_string(), serde_json::json!("ok")),
            ("source".to_string(), serde_json::json!("batch")),
        ],
    );

    assert_eq!(output["_row_count"], 2);
    assert_eq!(output["_rows"][0]["name"], "Alice");
    assert_eq!(output["_rows"][1]["name"], "Bob");
    assert_eq!(output["status"], "ok");
    assert_eq!(output["source"], "batch");
}

#[test]
fn test_from_input_value_uses_batch_frame_for_top_level_arrays() {
    let context = ExecutionContext::from_input_value(serde_json::json!([
        {"id": 1, "status": "new"},
        {"id": 2, "status": "done"}
    ]))
    .expect("top-level row arrays should build a batch-aware context");

    assert_eq!(context.input_data.as_array().unwrap().len(), 2);
    assert_eq!(context.get_batch_frame().unwrap().row_count(), 2);
}

#[test]
fn test_from_input_value_preserves_metadata_around_rows() {
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "job": "backfill",
        "_rows": [
            {"id": 1, "status": "new"},
            {"id": 2, "status": "done"}
        ]
    }))
    .expect("metadata-wrapped row payloads should remain compatible");

    assert_eq!(context.working_data["job"], "backfill");
    assert_eq!(context.get_batch_frame().unwrap().row_count(), 2);
}

#[test]
fn test_merge_step_output_refreshes_cached_batch_frame() {
    use crate::orchestration::workflow::runtime::frame::BatchFrame;

    let initial = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "new"}),
        serde_json::json!({"id": 2, "status": "done"}),
    ])
    .unwrap();

    let mut context = ExecutionContext::from_input_value(serde_json::json!({
        "job": "seed",
        "_rows": initial.to_json_values().unwrap(),
    }))
    .unwrap();
    context
        .merge_step_output(&serde_json::json!({
            "_rows": [
                {"id": 3, "status": "validated"},
                {"id": 4, "status": "archived"},
            ],
            "_row_count": 2,
            "_status": "updated"
        }))
        .unwrap();

    assert_eq!(context.working_data["_status"], "updated");
    let cached = context.get_batch_frame().unwrap();
    let rows = cached.to_json_values().unwrap();
    assert_eq!(rows[0]["id"], 3);
    assert_eq!(rows[1]["status"], "archived");
}

#[test]
fn test_get_context_batch_frame_prefers_cached_metadata() {
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "new"}),
        serde_json::json!({"id": 2, "status": "done"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_cached".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: None,
    });

    let context = ExecutionContext::from_batch_frame(frame).unwrap();
    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let rows = executor.get_rows_from_context(&context).unwrap();

    let cached = executor
        .get_context_batch_frame(&context, &rows)
        .unwrap()
        .expect("context-backed rows should reuse cached batch frame");

    assert_eq!(
        cached.metadata().source_step_id.as_deref(),
        Some("extract_cached")
    );
    assert_eq!(cached.metadata().source_kind.as_deref(), Some("db_extract"));
}

#[test]
fn test_try_with_context_batch_frame_passes_cached_frame_to_closure() {
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "new"}),
        serde_json::json!({"id": 2, "status": "done"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_cached".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: Some("datasource-1".to_string()),
    });

    let context = ExecutionContext::from_batch_frame(frame).unwrap();
    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let rows = executor.get_rows_from_context(&context).unwrap();

    let source_id = executor
        .try_with_context_batch_frame(&context, &rows, |batch| {
            Ok::<_, anyhow::Error>(batch.metadata().source_id.clone())
        })
        .unwrap();

    assert_eq!(source_id, Some(Some("datasource-1".to_string())));
}

#[test]
fn test_get_rows_from_context_falls_back_to_step_outputs_when_working_data_has_no_rows() {
    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut context = ExecutionContext::new(serde_json::json!({
        "job": "backfill",
        "_status": "seeded"
    }));
    context.step_outputs.insert(
        "extract_step".to_string(),
        serde_json::json!({
            "_rows": [
                {"id": 10, "status": "new"},
                {"id": 11, "status": "done"}
            ],
            "_row_count": 2
        }),
    );

    let rows = executor.get_rows_from_context(&context).unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], 10);
    assert_eq!(rows[1]["status"], "done");
}

#[test]
fn test_build_step_output_for_storage_strips_large_rows_only() {
    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let small_output = serde_json::json!({
        "_rows": [{"id": 1}],
        "_row_count": 1,
        "_status": "ok"
    });
    let large_output = serde_json::json!({
        "_rows": [{"id": 1}],
        "_row_count": 10001,
        "_status": "ok"
    });

    let stored_small = executor.build_step_output_for_storage("small_step", &small_output);
    let stored_large = executor.build_step_output_for_storage("large_step", &large_output);

    assert_eq!(stored_small, small_output);
    assert!(stored_large.get("_rows").is_none());
    assert_eq!(stored_large["_row_count"], 10001);
    assert_eq!(stored_large["_status"], "ok");
}

#[test]
fn test_merge_and_store_step_result_preserves_metadata_and_drops_frame() {
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "active"}),
        serde_json::json!({"id": 2, "status": "pending"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_step".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: Some("dataset_123".to_string()),
    });

    let step_result = StepResult {
        step_id: "transform_step".to_string(),
        success: true,
        output: serde_json::json!({
            "_rows": [
                {"id": 1, "status": "active"},
                {"id": 2, "status": "pending"}
            ],
            "_row_count": 2,
            "_status": "ok"
        }),
        confidence: 1.0,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
        batch_metadata: Some(frame.metadata().clone()),
        batch_frame: Some(frame),
    };

    let mut context = ExecutionContext::new(serde_json::json!({}));
    let mut step_results = HashMap::new();

    executor
        .merge_and_store_step_result(&mut context, &mut step_results, &step_result)
        .unwrap();

    assert_eq!(context.working_data["_row_count"], 2);
    assert!(context.working_data.get("_rows").is_some());

    let stored = step_results.get("transform_step").unwrap();
    assert!(stored.batch_frame.is_none());
    assert_eq!(
        stored
            .batch_metadata
            .as_ref()
            .and_then(|m| m.source_step_id.as_deref()),
        Some("extract_step")
    );
    assert_eq!(stored.output["_status"], "ok");
}

#[test]
fn test_data_validator_batch_path_preserves_legacy_output_shape() {
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, ValidationRule,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![
        serde_json::json!({"name": "Alice", "age": 30, "status": "active"}),
        serde_json::json!({"name": null, "age": 150, "status": "inactive"}),
        serde_json::json!({"name": "Bob", "age": 21, "status": "paused"}),
    ];
    let config = DataValidatorConfig {
        rules: vec![
            ValidationRule {
                field: "name".to_string(),
                rule_type: RuleType::NotNull,
                params: None,
                severity: Severity::Error,
            },
            ValidationRule {
                field: "age".to_string(),
                rule_type: RuleType::Range {
                    min: 0.0,
                    max: 120.0,
                },
                params: None,
                severity: Severity::Error,
            },
            ValidationRule {
                field: "status".to_string(),
                rule_type: RuleType::InSet {
                    values: vec!["active".to_string(), "inactive".to_string()],
                },
                params: None,
                severity: Severity::Warning,
            },
        ],
        fail_on_error: true,
    };
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "_rows": rows.clone()
    }))
    .unwrap();

    let result = executor
        .try_execute_data_validator_batch(&context, &config, &rows)
        .unwrap()
        .expect("supported row data should use batch validation path");

    assert!(!result.success);
    assert_eq!(result.output["_row_count"], 3);
    assert_eq!(result.output["_rows"], serde_json::json!(rows));
    assert_eq!(result.output["_error_count"], 2);
    assert_eq!(result.output["_warning_count"], 1);
    assert_eq!(result.confidence, 0.0);
    assert!(result.batch_frame.is_some());
}

#[test]
fn test_data_validator_batch_path_falls_back_for_unsupported_rows() {
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, ValidationRule,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![serde_json::json!("not an object row")];
    let config = DataValidatorConfig {
        rules: vec![ValidationRule {
            field: "name".to_string(),
            rule_type: RuleType::NotNull,
            params: None,
            severity: Severity::Error,
        }],
        fail_on_error: true,
    };
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": rows.clone()
    }));

    let result = executor
        .try_execute_data_validator_batch(&context, &config, &rows)
        .unwrap();
    assert!(
        result.is_none(),
        "non-object rows should cleanly decline the batch fast path"
    );
}

#[test]
fn test_aggregator_batch_path_preserves_legacy_output_shape() {
    use crate::orchestration::workflow::definition::{AggFunction, Aggregation, AggregatorConfig};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![
        serde_json::json!({"region": "east", "amount": 10.0, "orders": 1}),
        serde_json::json!({"region": "east", "amount": 15.0, "orders": 2}),
        serde_json::json!({"region": "west", "amount": 7.0, "orders": 3}),
    ];
    let config = AggregatorConfig {
        group_by: vec!["region".to_string()],
        aggregations: vec![
            Aggregation {
                field: "amount".to_string(),
                function: AggFunction::Sum,
                alias: Some("total_amount".to_string()),
            },
            Aggregation {
                field: "orders".to_string(),
                function: AggFunction::Count,
                alias: Some("order_count".to_string()),
            },
        ],
    };
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "_rows": rows.clone()
    }))
    .unwrap();

    let result = executor
        .try_execute_aggregator_batch(&context, &config, &rows)
        .unwrap()
        .expect("supported row data should use batch aggregation path");

    assert!(result.success);
    assert_eq!(result.output["_row_count"], 2);
    assert_eq!(result.output["_original_count"], 3);
    assert_eq!(result.confidence, 1.0);
    assert!(result.batch_frame.is_some());
    let output_rows = result.output["_rows"].as_array().unwrap();
    assert_eq!(output_rows.len(), 2);
    assert!(output_rows.iter().any(|row| {
        row["region"] == "\"east\"" && row["total_amount"] == 25.0 && row["order_count"] == 2.0
    }));
    assert!(output_rows.iter().any(|row| {
        row["region"] == "\"west\"" && row["total_amount"] == 7.0 && row["order_count"] == 1.0
    }));
}

#[test]
fn test_aggregator_batch_path_falls_back_for_unsupported_rows() {
    use crate::orchestration::workflow::definition::{AggFunction, Aggregation, AggregatorConfig};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![serde_json::json!("not an object row")];
    let config = AggregatorConfig {
        group_by: vec!["region".to_string()],
        aggregations: vec![Aggregation {
            field: "amount".to_string(),
            function: AggFunction::Sum,
            alias: Some("total_amount".to_string()),
        }],
    };
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": rows.clone()
    }));

    let result = executor
        .try_execute_aggregator_batch(&context, &config, &rows)
        .unwrap();
    assert!(
        result.is_none(),
        "non-object rows should cleanly decline the batch aggregation path"
    );
}

#[test]
fn test_deduplicator_batch_path_preserves_legacy_output_shape_for_exact_first() {
    use crate::orchestration::workflow::definition::{
        DedupMethod, DeduplicatorConfig, KeepStrategy,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![
        serde_json::json!({"id": 1, "name": "Alice"}),
        serde_json::json!({"id": 1, "name": "Alice Updated"}),
        serde_json::json!({"id": 2, "name": "Bob"}),
    ];
    let config = DeduplicatorConfig {
        method: DedupMethod::Exact,
        key_fields: vec!["id".to_string()],
        threshold: None,
        keep: KeepStrategy::First,
    };
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "_rows": rows.clone()
    }))
    .unwrap();

    let result = executor
        .try_execute_deduplicator_batch(&context, &config, &rows)
        .unwrap()
        .expect("exact+first dedup rows should use batch fast path");

    assert!(result.success);
    assert_eq!(result.output["_row_count"], 2);
    assert_eq!(result.output["_original_count"], 3);
    assert_eq!(result.output["_duplicates_removed"], 1);
    assert_eq!(result.confidence, 1.0);
    assert!(result.batch_frame.is_some());
    let output_rows = result.output["_rows"].as_array().unwrap();
    assert_eq!(output_rows[0]["name"], "Alice");
    assert_eq!(output_rows[1]["name"], "Bob");
    let modifications = result.output["_modifications"].as_array().unwrap();
    assert_eq!(modifications.len(), 1);
    assert_eq!(
        modifications[0]["metadata"]["execution_path"],
        "batch_frame"
    );
}

#[test]
fn test_deduplicator_batch_path_declines_for_unsupported_strategy() {
    use crate::orchestration::workflow::definition::{
        DedupMethod, DeduplicatorConfig, KeepStrategy,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![
        serde_json::json!({"id": 1, "name": "Alice"}),
        serde_json::json!({"id": 1, "name": "Alice Updated"}),
    ];
    let config = DeduplicatorConfig {
        method: DedupMethod::Exact,
        key_fields: vec!["id".to_string()],
        threshold: None,
        keep: KeepStrategy::Last,
    };
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "_rows": rows.clone()
    }))
    .unwrap();

    let result = executor
        .try_execute_deduplicator_batch(&context, &config, &rows)
        .unwrap();
    assert!(
        result.is_none(),
        "non-first keep strategies should stay on the legacy dedup path"
    );
}

#[test]
fn test_field_transformer_batch_path_preserves_row_output_shape() {
    use crate::orchestration::workflow::definition::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![
        serde_json::json!({"email": "  TEST@EXAMPLE.COM  ", "status": "ACTIVE"}),
        serde_json::json!({"email": "  SECOND@EXAMPLE.COM  ", "status": "PENDING"}),
    ];
    let config = FieldTransformerConfig {
        transformations: vec![
            FieldTransformation {
                field: "email".to_string(),
                operations: vec![TransformOperation::Trim, TransformOperation::Lower],
            },
            FieldTransformation {
                field: "status".to_string(),
                operations: vec![TransformOperation::Lower],
            },
        ],
    };
    let context = ExecutionContext::from_input_value(serde_json::json!({
        "_rows": rows.clone()
    }))
    .unwrap();

    let result = executor
        .try_execute_field_transformer_batch(&context, &config, &rows)
        .unwrap()
        .expect("supported row data should use batch field-transform path");

    assert!(result.success);
    assert_eq!(result.output["_row_count"], 2);
    assert_eq!(result.output["_rows_transformed"], 2);
    assert_eq!(result.output["_fields_modified"], 4);
    assert_eq!(result.confidence, 1.0);
    assert_eq!(result.output["_rows"][0]["email"], "test@example.com");
    assert_eq!(result.output["_rows"][1]["status"], "pending");
    assert!(result.batch_frame.is_some());
    let modifications = result.output["_modifications"].as_array().unwrap();
    assert_eq!(modifications.len(), 2);
    assert!(modifications.iter().any(|modification| {
        modification["field_name"] == "email" && modification["metadata"]["rows_modified"] == 2
    }));
    assert!(modifications.iter().any(|modification| {
        modification["field_name"] == "status" && modification["metadata"]["rows_modified"] == 2
    }));
}

#[test]
fn test_field_transformer_batch_path_falls_back_for_unsupported_rows() {
    use crate::orchestration::workflow::definition::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = vec![serde_json::json!("not an object row")];
    let config = FieldTransformerConfig {
        transformations: vec![FieldTransformation {
            field: "email".to_string(),
            operations: vec![TransformOperation::Lower],
        }],
    };
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": rows.clone()
    }));

    let result = executor
        .try_execute_field_transformer_batch(&context, &config, &rows)
        .unwrap();
    assert!(
        result.is_none(),
        "non-object rows should cleanly decline the batch field-transform path"
    );
}

#[test]
fn test_merge_step_output_with_batch_preserves_supplied_metadata() {
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let mut context = ExecutionContext::new(serde_json::json!({
        "job": "seed",
        "_rows": [{"id": 1, "status": "new"}]
    }));
    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 2, "status": "trimmed"}),
        serde_json::json!({"id": 3, "status": "lowered"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("field_transform".to_string()),
        source_kind: Some("field_transformer".to_string()),
        source_id: None,
    });

    context
        .merge_step_output_with_batch(
            &serde_json::json!({
                "_rows": [
                    {"id": 2, "status": "trimmed"},
                    {"id": 3, "status": "lowered"}
                ],
                "_row_count": 2
            }),
            Some(frame),
        )
        .unwrap();

    let cached = context.get_batch_frame().unwrap();
    assert_eq!(
        cached.metadata().source_step_id.as_deref(),
        Some("field_transform")
    );
    assert_eq!(
        cached.metadata().source_kind.as_deref(),
        Some("field_transformer")
    );
    assert_eq!(context.working_data["_row_count"], 2);
}

#[test]
fn test_set_batch_frame_metadata_updates_cached_frame_without_rebuilding_rows() {
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "new"}),
        serde_json::json!({"id": 2, "status": "done"}),
    ])
    .unwrap();

    let mut context = ExecutionContext::from_batch_frame(frame).unwrap();
    context.set_batch_frame_metadata(BatchFrameMetadata {
        source_step_id: Some("dataset_input".to_string()),
        source_kind: Some("dataset".to_string()),
        source_id: Some("dataset_123".to_string()),
    });

    let cached = context.get_batch_frame().unwrap();
    assert_eq!(cached.row_count(), 2);
    assert_eq!(
        cached.metadata().source_step_id.as_deref(),
        Some("dataset_input")
    );
    assert_eq!(cached.metadata().source_kind.as_deref(), Some("dataset"));
    assert_eq!(cached.metadata().source_id.as_deref(), Some("dataset_123"));
}
