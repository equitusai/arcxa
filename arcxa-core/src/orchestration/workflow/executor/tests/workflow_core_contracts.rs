use super::*;
use crate::orchestration::workflow::definition::{
    ConfidenceAggregateConfig, ConfidenceGateConfig, DataJoinerConfig, FallbackStrategy,
    HeuristicConfig, JoinType, RdfLoaderConfig, StepConfig, StepType, WasmRuleConfig,
    WeightedVoteConfig, WorkflowDefinition, WorkflowStep,
};

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

#[tokio::test]
async fn test_executor_creation() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let _executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
}

#[tokio::test]
async fn test_execute_workflow() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({"confidence": 0.6}));
    let result = executor.execute(context).await.unwrap();

    assert!(result.success);
    assert_eq!(result.step_results.len(), 1);
}

#[tokio::test]
async fn test_confidence_gate() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut context = ExecutionContext::new(serde_json::json!({}));
    context.step_outputs.insert(
        "previous_step".to_string(),
        serde_json::json!({"confidence": 0.9}),
    );

    let result = executor.execute(context).await.unwrap();
    assert!(result.success);
}

#[tokio::test]
async fn test_execute_weighted_vote_combines_step_confidences() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut context = ExecutionContext::new(serde_json::json!({}));
    context.step_outputs.insert(
        "ml_step".to_string(),
        serde_json::json!({"confidence": 0.8}),
    );
    context.step_outputs.insert(
        "rules_step".to_string(),
        serde_json::json!({"confidence": 0.2}),
    );

    let config = WeightedVoteConfig {
        weights: HashMap::from([
            ("ml_step".to_string(), 0.75),
            ("rules_step".to_string(), 0.25),
        ]),
    };

    let (success, output, confidence) = executor
        .execute_weighted_vote(&config, &context)
        .await
        .unwrap();

    assert!(success);
    assert!((confidence - 0.65).abs() < f64::EPSILON);
    let weighted_confidence = output
        .get("weighted_confidence")
        .and_then(|value| value.as_f64())
        .unwrap();
    assert!((weighted_confidence - 0.65).abs() < 1e-12);
}

#[tokio::test]
async fn test_execute_confidence_aggregate_uses_declared_inputs() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut context = ExecutionContext::new(serde_json::json!({}));
    context.step_outputs.insert(
        "source_a".to_string(),
        serde_json::json!({"confidence": 0.2}),
    );
    context.step_outputs.insert(
        "source_b".to_string(),
        serde_json::json!({"confidence": 0.8}),
    );
    context.step_outputs.insert(
        "ignored_source".to_string(),
        serde_json::json!({"confidence": 1.0}),
    );

    let config = ConfidenceAggregateConfig {
        method: "weighted_average".to_string(),
        inputs: vec!["source_a".to_string(), "source_b".to_string()],
    };

    let (success, output, confidence) = executor
        .execute_confidence_aggregate(&config, &context)
        .await
        .unwrap();

    assert!(success);
    assert!((confidence - 0.5).abs() < f64::EPSILON);
    assert_eq!(
        output,
        serde_json::json!({
            "method": "weighted_average",
            "aggregated_confidence": 0.5,
            "input_count": 2
        })
    );
}

#[tokio::test]
async fn test_execute_heuristic_returns_contextual_error_for_missing_rule() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({"field": "value"}));
    let config = HeuristicConfig {
        rule_id: "missing_rule".to_string(),
        min_confidence: 0.5,
    };

    let error = executor
        .execute_heuristic(&config, &context)
        .await
        .expect_err("missing heuristic rule should surface an execution error");

    assert!(
        error
            .to_string()
            .contains("Heuristic rule execution failed"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn test_execute_wasm_rule_returns_contextual_error_for_missing_rule() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({"field": "value"}));
    let config = WasmRuleConfig {
        rule_id: "missing_wasm_rule".to_string(),
    };

    let error = executor
        .execute_wasm_rule(&config, &context)
        .await
        .expect_err("missing wasm rule should surface an execution error");

    assert!(
        error.to_string().contains("WASM rule execution failed"),
        "unexpected error: {error:#}"
    );
}

#[tokio::test]
async fn test_execute_data_joiner_returns_stub_output_shape() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({}));
    let config = DataJoinerConfig {
        join_type: JoinType::Left,
        left_key: vec!["customer_id".to_string()],
        right_key: vec!["id".to_string()],
        output_columns: None,
    };

    let (success, output, confidence) = executor
        .execute_data_joiner(&config, &context)
        .await
        .expect("stub data joiner should execute successfully");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(
        output,
        serde_json::json!({
            "_join_type": "Left",
            "_left_key": ["customer_id"],
            "_right_key": ["id"],
            "_status": "stub_implementation",
            "_rows": [],
            "_row_count": 0,
        })
    );
}

#[tokio::test]
async fn test_execute_rdf_loader_returns_stub_output_shape() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": "cust-1", "name": "Alice"},
            {"id": "cust-2", "name": "Bob"}
        ]
    }));
    let config = RdfLoaderConfig {
        target_graph: Some("urn:arcxa:test".to_string()),
        entity_type: "Customer".to_string(),
        id_field: "id".to_string(),
        batch_size: 1000,
        capture_lineage: true,
    };

    let (success, output, confidence) = executor
        .execute_rdf_loader(&config, &context)
        .await
        .expect("stub RDF loader should execute successfully");

    assert!(success);
    assert_eq!(confidence, 1.0);
    assert_eq!(
        output,
        serde_json::json!({
            "_entity_type": "Customer",
            "_id_field": "id",
            "_target_graph": "urn:arcxa:test",
            "_status": "stub_implementation",
            "_rows_to_load": 2,
        })
    );
}

#[test]
fn test_resolve_feature_reads_nested_step_output_paths() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let mut context = ExecutionContext::new(serde_json::json!({
        "email": "person@example.com"
    }));
    context.step_outputs.insert(
        "score_step".to_string(),
        serde_json::json!({
            "output": {
                "score": 0.91,
                "band": "gold"
            }
        }),
    );

    let value = executor
        .resolve_feature("score_step.output.score", &context)
        .expect("nested step output path should resolve");

    assert_eq!(value, serde_json::json!(0.91));
}

#[test]
fn test_extract_features_from_mappings_applies_transform_after_resolution() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::FeatureMapping;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "  USER@EXAMPLE.COM  "
    }));
    let mappings = vec![FeatureMapping {
        feature_name: "normalized_email".to_string(),
        field_name: "email".to_string(),
        transform: Some("trim".to_string()),
    }];

    let features = executor
        .extract_features_from_mappings(&mappings, &context)
        .expect("feature mappings should resolve and transform");

    assert_eq!(
        features.get("normalized_email"),
        Some(&serde_json::json!("USER@EXAMPLE.COM"))
    );
}

#[tokio::test]
async fn test_execute_field_transformer_falls_back_to_legacy_object_transform() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        FieldTransformation, FieldTransformerConfig, TransformOperation,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let config = FieldTransformerConfig {
        transformations: vec![FieldTransformation {
            field: "email".to_string(),
            operations: vec![TransformOperation::Trim, TransformOperation::Lower],
        }],
    };
    let context = ExecutionContext::new(serde_json::json!({"email": "  TEST@EXAMPLE.COM  "}));

    let result = executor
        .execute_field_transformer(&config, &context)
        .await
        .expect("legacy object transform path should execute");

    assert!(result.success);
    assert_eq!(result.confidence, 1.0);
    assert!(result.batch_frame.is_none());
    assert_eq!(
        result.output["email"],
        serde_json::json!("test@example.com")
    );
    assert_eq!(result.output["_fields_modified"], serde_json::json!(1));
    assert_eq!(result.output["_modifications"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_execute_step_field_transformer_attaches_batch_frame_sidecar() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        FieldTransformation, FieldTransformerConfig, StepConfig, StepType, TransformOperation,
        WorkflowStep,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "transform_step".to_string(),
        step_type: StepType::FieldTransformer,
        config: StepConfig::FieldTransformer(FieldTransformerConfig {
            transformations: vec![FieldTransformation {
                field: "status".to_string(),
                operations: vec![TransformOperation::Lower],
            }],
        }),
        depends_on: vec![],
    };
    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "ACTIVE"}),
        serde_json::json!({"id": 2, "status": "PENDING"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_step".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: None,
    });
    let context = ExecutionContext::from_batch_frame(frame).unwrap();

    let step_result = executor.execute_step(&step, &context).await.unwrap();

    assert!(step_result.success);
    assert_eq!(step_result.output["_rows"][0]["status"], "active");
    let batch_frame = step_result
        .batch_frame
        .expect("field transformer batch path should attach a frame sidecar");
    assert_eq!(
        batch_frame.metadata().source_step_id.as_deref(),
        Some("extract_step")
    );
    assert_eq!(
        batch_frame.metadata().source_kind.as_deref(),
        Some("db_extract")
    );
}

#[tokio::test]
async fn test_execute_step_data_validator_attaches_batch_frame_sidecar() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, StepConfig, StepType, ValidationRule, WorkflowStep,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "validate_step".to_string(),
        step_type: StepType::DataValidator,
        config: StepConfig::DataValidator(DataValidatorConfig {
            rules: vec![ValidationRule {
                field: "status".to_string(),
                rule_type: RuleType::InSet {
                    values: vec!["active".to_string(), "pending".to_string()],
                },
                params: None,
                severity: Severity::Error,
            }],
            fail_on_error: false,
        }),
        depends_on: vec![],
    };
    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "status": "active"}),
        serde_json::json!({"id": 2, "status": "archived"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_step".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: None,
    });
    let context = ExecutionContext::from_batch_frame(frame).unwrap();

    let step_result = executor.execute_step(&step, &context).await.unwrap();

    assert!(step_result.success);
    assert_eq!(step_result.output["_error_count"], 1);
    let batch_frame = step_result
        .batch_frame
        .expect("data validator batch path should attach a frame sidecar");
    assert_eq!(
        batch_frame.metadata().source_step_id.as_deref(),
        Some("extract_step")
    );
    assert_eq!(
        batch_frame.metadata().source_kind.as_deref(),
        Some("db_extract")
    );
}

#[tokio::test]
async fn test_execute_data_validator_falls_back_to_legacy_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, ValidationRule,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [123, 456]
    }));
    let config = DataValidatorConfig {
        rules: vec![ValidationRule {
            field: "name".to_string(),
            rule_type: RuleType::NotNull,
            params: None,
            severity: Severity::Error,
        }],
        fail_on_error: true,
    };

    let result = executor
        .execute_data_validator(&config, &context)
        .await
        .expect("legacy validator path should execute");

    assert!(!result.success);
    assert!(result.batch_frame.is_none());
    assert_eq!(result.confidence, 0.0);
    assert_eq!(result.output["_row_count"], serde_json::json!(2));
    assert_eq!(result.output["_error_count"], serde_json::json!(2));
    assert_eq!(result.output["_warning_count"], serde_json::json!(0));
    assert_eq!(result.output["_rows"], serde_json::json!([123, 456]));
}

#[tokio::test]
async fn test_execute_data_validator_legacy_object_rows_attach_batch_frame() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DataValidatorConfig, RuleType, Severity, ValidationRule,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"name": "Alice"},
            {"id": 2}
        ]
    }));
    let config = DataValidatorConfig {
        rules: vec![ValidationRule {
            field: "name".to_string(),
            rule_type: RuleType::NotNull,
            params: None,
            severity: Severity::Error,
        }],
        fail_on_error: false,
    };

    let result = executor
        .execute_data_validator(&config, &context)
        .await
        .expect("legacy object-row validator path should execute");

    assert!(result.success);
    assert_eq!(result.output["_row_count"], serde_json::json!(2));
    assert_eq!(result.output["_error_count"], serde_json::json!(1));
    let batch_frame = result
        .batch_frame
        .expect("object-row validator output should attach a frame sidecar");
    assert_eq!(batch_frame.row_count(), 2);
}

#[tokio::test]
async fn test_execute_step_aggregator_attaches_batch_frame_sidecar() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        AggFunction, Aggregation, AggregatorConfig, StepConfig, StepType, WorkflowStep,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "aggregate_step".to_string(),
        step_type: StepType::Aggregator,
        config: StepConfig::Aggregator(AggregatorConfig {
            group_by: vec!["region".to_string()],
            aggregations: vec![Aggregation {
                field: "amount".to_string(),
                function: AggFunction::Sum,
                alias: Some("total_amount".to_string()),
            }],
        }),
        depends_on: vec![],
    };
    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"region": "east", "amount": 10.0}),
        serde_json::json!({"region": "east", "amount": 15.0}),
        serde_json::json!({"region": "west", "amount": 7.0}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_step".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: None,
    });
    let context = ExecutionContext::from_batch_frame(frame).unwrap();

    let step_result = executor.execute_step(&step, &context).await.unwrap();

    assert!(step_result.success);
    assert_eq!(step_result.output["_row_count"], 2);
    let batch_frame = step_result
        .batch_frame
        .expect("aggregator batch path should attach a frame sidecar");
    assert_eq!(
        batch_frame.metadata().source_step_id.as_deref(),
        Some("extract_step")
    );
    assert_eq!(
        batch_frame.metadata().source_kind.as_deref(),
        Some("db_extract")
    );
}

#[tokio::test]
async fn test_execute_aggregator_falls_back_to_legacy_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{AggFunction, Aggregation, AggregatorConfig};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [123, 456]
    }));
    let config = AggregatorConfig {
        group_by: vec!["region".to_string()],
        aggregations: vec![Aggregation {
            field: "amount".to_string(),
            function: AggFunction::Sum,
            alias: Some("total_amount".to_string()),
        }],
    };

    let result = executor
        .execute_aggregator(&config, &context)
        .await
        .expect("legacy aggregator path should execute");

    assert!(result.success);
    let batch_frame = result
        .batch_frame
        .expect("legacy aggregator object-row output should attach a frame sidecar");
    assert_eq!(batch_frame.row_count(), 1);
    assert_eq!(result.confidence, 1.0);
    assert_eq!(result.output["_row_count"], serde_json::json!(1));
    assert_eq!(result.output["_original_count"], serde_json::json!(2));
    assert_eq!(result.output["_rows"][0]["region"], "");
    assert_eq!(result.output["_rows"][0]["total_amount"], 0.0);
}

#[tokio::test]
async fn test_execute_step_deduplicator_attaches_batch_frame_sidecar() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DedupMethod, DeduplicatorConfig, KeepStrategy, StepConfig, StepType, WorkflowStep,
    };
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "dedup_step".to_string(),
        step_type: StepType::Deduplicator,
        config: StepConfig::Deduplicator(DeduplicatorConfig {
            method: DedupMethod::Exact,
            key_fields: vec!["id".to_string()],
            threshold: None,
            keep: KeepStrategy::First,
        }),
        depends_on: vec![],
    };
    let frame = BatchFrame::from_json_values(&[
        serde_json::json!({"id": 1, "name": "Alice"}),
        serde_json::json!({"id": 1, "name": "Alice Updated"}),
        serde_json::json!({"id": 2, "name": "Bob"}),
    ])
    .unwrap()
    .with_metadata(BatchFrameMetadata {
        source_step_id: Some("extract_step".to_string()),
        source_kind: Some("db_extract".to_string()),
        source_id: None,
    });
    let context = ExecutionContext::from_batch_frame(frame).unwrap();

    let step_result = executor.execute_step(&step, &context).await.unwrap();

    assert!(step_result.success);
    assert_eq!(step_result.output["_duplicates_removed"], 1);
    let batch_frame = step_result
        .batch_frame
        .expect("deduplicator batch path should attach a frame sidecar");
    assert_eq!(
        batch_frame.metadata().source_step_id.as_deref(),
        Some("extract_step")
    );
    assert_eq!(
        batch_frame.metadata().source_kind.as_deref(),
        Some("db_extract")
    );
}

#[tokio::test]
async fn test_execute_deduplicator_falls_back_to_legacy_output_contract() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::{
        DedupMethod, DeduplicatorConfig, KeepStrategy,
    };

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "_rows": [
            {"id": 1, "name": "Alice"},
            {"id": 1, "name": "Alice Updated"},
            {"id": 2, "name": "Bob"}
        ]
    }));
    let config = DeduplicatorConfig {
        method: DedupMethod::Exact,
        key_fields: vec!["id".to_string()],
        threshold: None,
        keep: KeepStrategy::Last,
    };

    let result = executor
        .execute_deduplicator(&config, &context)
        .await
        .expect("legacy deduplicator path should execute");

    assert!(result.success);
    let batch_frame = result
        .batch_frame
        .expect("legacy deduplicator object-row output should attach a frame sidecar");
    assert_eq!(batch_frame.row_count(), 2);
    assert_eq!(result.confidence, 1.0);
    assert_eq!(result.output["_row_count"], serde_json::json!(2));
    assert_eq!(result.output["_original_count"], serde_json::json!(3));
    assert_eq!(result.output["_duplicates_removed"], serde_json::json!(1));
    let rows = result.output["_rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().any(|row| {
        row["id"] == serde_json::json!(1) && row["name"] == serde_json::json!("Alice Updated")
    }));
    assert!(rows
        .iter()
        .any(|row| row["id"] == serde_json::json!(2) && row["name"] == serde_json::json!("Bob")));
}

#[tokio::test]
async fn test_data_flow_between_steps() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.5,
                    input_step: None,
                }),
                depends_on: vec![],
            },
            WorkflowStep {
                id: "step2".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.6,
                    input_step: Some("step1".to_string()),
                }),
                depends_on: vec!["step1".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({
        "confidence": 0.75
    }));

    let result = executor.execute(context).await.unwrap();

    assert!(result.success);
    assert_eq!(result.step_results.len(), 2);

    let step1_result = result.step_results.get("step1").unwrap();
    assert!(step1_result.success);
    assert_eq!(step1_result.confidence, 0.75);

    let step2_result = result.step_results.get("step2").unwrap();
    assert!(step2_result.success);
    assert_eq!(step2_result.confidence, 0.75);
}

#[tokio::test]
async fn test_working_data_propagation() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "gate1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.5,
                    input_step: None,
                }),
                depends_on: vec![],
            },
            WorkflowStep {
                id: "gate2".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.7,
                    input_step: Some("gate1".to_string()),
                }),
                depends_on: vec!["gate1".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let context = ExecutionContext::new(serde_json::json!({
        "confidence": 0.85,
        "entity_id": "test_123"
    }));

    let result = executor.execute(context).await.unwrap();

    assert!(result.success);

    let gate1 = result.step_results.get("gate1").unwrap();
    assert!(gate1.success);
    assert_eq!(gate1.confidence, 0.85);

    let gate2 = result.step_results.get("gate2").unwrap();
    assert!(gate2.success);
    assert_eq!(gate2.confidence, 0.85);

    assert!(gate1.output.get("confidence").is_some());
    assert!(gate2.output.get("confidence").is_some());
}
