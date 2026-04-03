use super::*;
use crate::orchestration::workflow::definition::{
    ConfidenceGateConfig, FallbackStrategy, MLPredictionConfig, PredictionSpec, StepConfig,
    StepType, WorkflowDefinition, WorkflowStep,
};
use crate::orchestration::workflow::lineage_tracker::MLPredictionStepRecord;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

fn build_large_result_rows(count: usize) -> Vec<serde_json::Value> {
    (0..count)
        .map(|idx| serde_json::json!({"id": idx, "status": "done"}))
        .collect()
}

#[derive(Default)]
struct TestLineageTracker {
    workflow_starts: Mutex<Vec<WorkflowExecutionRecord>>,
    step_records: Mutex<Vec<StepExecutionRecord>>,
    ml_records: Mutex<Vec<MLPredictionStepRecord>>,
    workflow_completions: Mutex<Vec<(String, bool)>>,
}

#[async_trait::async_trait]
impl LineageTracker for TestLineageTracker {
    async fn record_workflow_start(&self, record: WorkflowExecutionRecord) -> anyhow::Result<()> {
        self.workflow_starts.lock().unwrap().push(record);
        Ok(())
    }

    async fn record_step_execution(&self, record: StepExecutionRecord) -> anyhow::Result<()> {
        self.step_records.lock().unwrap().push(record);
        Ok(())
    }

    async fn record_ml_predictions(&self, record: MLPredictionStepRecord) -> anyhow::Result<()> {
        self.ml_records.lock().unwrap().push(record);
        Ok(())
    }

    async fn record_workflow_complete(
        &self,
        execution_id: String,
        success: bool,
        _completed_at: chrono::DateTime<chrono::Utc>,
    ) -> anyhow::Result<()> {
        self.workflow_completions
            .lock()
            .unwrap()
            .push((execution_id, success));
        Ok(())
    }
}

#[test]
fn test_build_step_execution_record_extracts_modifications() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let step = WorkflowStep {
        id: "transform_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: true,
        output: serde_json::json!({
            "_modifications": [
                {
                    "field_name": "email",
                    "old_value": "A@Example.COM",
                    "new_value": "a@example.com",
                    "is_reversible": true,
                    "operations": 2
                }
            ]
        }),
        confidence: 0.9,
        started_at,
        completed_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };

    let record = executor.build_step_execution_record("exec_1", &step, &step_result);

    assert_eq!(record.execution_id, "exec_1");
    assert_eq!(record.step_id, "transform_step");
    assert_eq!(record.step_type, StepType::ConfidenceGate.to_string());
    assert_eq!(record.started_at, started_at);
    assert_eq!(record.completed_at, completed_at);
    assert_eq!(record.modifications.len(), 1);
    assert_eq!(record.modifications[0].field_name, "email");
    assert_eq!(record.modifications[0].old_value, "A@Example.COM");
    assert_eq!(record.modifications[0].new_value, "a@example.com");
    assert!(record.modifications[0].is_reversible);
    assert_eq!(record.modifications[0].operation_count, 2);
}

#[test]
fn test_extract_modifications_skips_entries_without_field_name() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let modifications = executor.extract_modifications(&serde_json::json!({
        "_modifications": [
            {
                "old_value": "ignored",
                "new_value": "still_ignored",
                "operations": 4
            },
            {
                "field_name": "status",
                "old_value": "NEW",
                "new_value": "normalized",
                "is_reversible": false,
                "operations": 1
            }
        ]
    }));

    assert_eq!(modifications.len(), 1);
    assert_eq!(modifications[0].field_name, "status");
    assert_eq!(modifications[0].old_value, "NEW");
    assert_eq!(modifications[0].new_value, "normalized");
    assert!(!modifications[0].is_reversible);
    assert_eq!(modifications[0].operation_count, 1);
}

#[test]
fn test_build_ml_prediction_step_record_extracts_predictions() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let step = WorkflowStep {
        id: "ml_step".to_string(),
        step_type: StepType::MlPrediction,
        config: StepConfig::MLPrediction(MLPredictionConfig {
            model_id: "customer-segmentation".to_string(),
            model_version: "2026.03".to_string(),
            features: vec!["email".to_string()],
            feature_mappings: vec![],
            predictions: vec![PredictionSpec {
                attribute_name: "segment".to_string(),
                mock_value: "enterprise".to_string(),
                mock_confidence: 0.92,
            }],
            confidence_threshold: Some(0.8),
            timeout_ms: 500,
            cache_ttl_secs: Some(60),
        }),
        depends_on: vec![],
    };
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: true,
        output: serde_json::json!({
            "_predictions": [
                {
                    "attribute_name": "segment",
                    "value": "enterprise",
                    "confidence": 0.92
                }
            ]
        }),
        confidence: 0.92,
        started_at,
        completed_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };

    let record = executor
        .build_ml_prediction_step_record("exec_ml", &step, &step_result)
        .expect("ml prediction step should yield a lineage record");

    assert_eq!(record.execution_id, "exec_ml");
    assert_eq!(record.step_id, "ml_step");
    assert_eq!(record.model_id, "customer-segmentation");
    assert_eq!(record.model_version, "2026.03");
    assert_eq!(record.started_at, started_at);
    assert_eq!(record.completed_at, completed_at);
    assert_eq!(record.predictions.len(), 1);
    assert_eq!(record.predictions[0].attribute_name, "segment");
    assert_eq!(record.predictions[0].value, "enterprise");
    assert_eq!(record.predictions[0].confidence, 0.92);
}

#[test]
fn test_extract_predictions_skips_entries_without_attribute_name() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let config = StepConfig::MLPrediction(MLPredictionConfig {
        model_id: "customer-segmentation".to_string(),
        model_version: "2026.03".to_string(),
        features: vec!["email".to_string()],
        feature_mappings: vec![],
        predictions: vec![PredictionSpec {
            attribute_name: "segment".to_string(),
            mock_value: "enterprise".to_string(),
            mock_confidence: 0.92,
        }],
        confidence_threshold: Some(0.8),
        timeout_ms: 500,
        cache_ttl_secs: Some(60),
    });

    let extracted = executor
        .extract_predictions(
            &serde_json::json!({
                "_predictions": [
                    {
                        "value": "ignored",
                        "confidence": 0.13
                    },
                    {
                        "attribute_name": "segment",
                        "value": "enterprise",
                        "confidence": 0.92
                    }
                ]
            }),
            &config,
        )
        .expect("at least one valid prediction should be extracted");

    assert_eq!(extracted.model_id, "customer-segmentation");
    assert_eq!(extracted.model_version, "2026.03");
    assert_eq!(extracted.predictions.len(), 1);
    assert_eq!(extracted.predictions[0].attribute_name, "segment");
    assert_eq!(extracted.predictions[0].value, "enterprise");
    assert_eq!(extracted.predictions[0].confidence, 0.92);
}

#[test]
fn test_generate_mock_prediction_returns_configured_mock_value() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let prediction = executor
        .generate_mock_prediction(
            &PredictionSpec {
                attribute_name: "segment".to_string(),
                mock_value: "enterprise".to_string(),
                mock_confidence: 0.92,
            },
            &HashMap::from([("email".to_string(), serde_json::json!("a@example.com"))]),
            &ExecutionContext::new(serde_json::json!({})),
        )
        .expect("configured mock value should be returned directly");

    assert_eq!(prediction, serde_json::json!("enterprise"));
}

#[tokio::test]
async fn test_execute_ml_prediction_emits_features_and_prediction_metadata() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::definition::FeatureMapping;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let config = MLPredictionConfig {
        model_id: "customer-segmentation".to_string(),
        model_version: "2026.03".to_string(),
        features: vec![],
        feature_mappings: vec![FeatureMapping {
            feature_name: "normalized_email".to_string(),
            field_name: "email".to_string(),
            transform: Some("trim".to_string()),
        }],
        predictions: vec![PredictionSpec {
            attribute_name: "segment".to_string(),
            mock_value: "enterprise".to_string(),
            mock_confidence: 0.92,
        }],
        confidence_threshold: Some(0.8),
        timeout_ms: 500,
        cache_ttl_secs: Some(60),
    };
    let context = ExecutionContext::new(serde_json::json!({
        "email": "  USER@EXAMPLE.COM  "
    }));

    let (success, output, confidence) = executor
        .execute_ml_prediction(&config, &context)
        .await
        .expect("ml prediction execution should succeed");

    assert!(success);
    assert_eq!(confidence, 0.92);
    assert_eq!(output["segment"], "enterprise");
    assert_eq!(output["_model_id"], "customer-segmentation");
    assert_eq!(output["_model_version"], "2026.03");
    assert_eq!(
        output["_features_used"]["normalized_email"],
        "USER@EXAMPLE.COM"
    );
    assert_eq!(output["_predictions"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn test_record_step_lineage_routes_standard_and_ml_records() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let tracker = Arc::new(TestLineageTracker::default());
    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone()).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);

    let standard_step = WorkflowStep {
        id: "transform_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let standard_result = StepResult {
        step_id: standard_step.id.clone(),
        success: true,
        output: serde_json::json!({
            "_modifications": [
                {
                    "field_name": "status",
                    "old_value": "NEW",
                    "new_value": "normalized",
                    "is_reversible": false,
                    "operations": 1
                }
            ]
        }),
        confidence: 1.0,
        started_at,
        completed_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };

    executor
        .record_step_lineage("exec_lineage", &standard_step, &standard_result)
        .await;

    let ml_step = WorkflowStep {
        id: "ml_step".to_string(),
        step_type: StepType::MlPrediction,
        config: StepConfig::MLPrediction(MLPredictionConfig {
            model_id: "customer-segmentation".to_string(),
            model_version: "2026.03".to_string(),
            features: vec!["email".to_string()],
            feature_mappings: vec![],
            predictions: vec![PredictionSpec {
                attribute_name: "segment".to_string(),
                mock_value: "enterprise".to_string(),
                mock_confidence: 0.92,
            }],
            confidence_threshold: Some(0.8),
            timeout_ms: 500,
            cache_ttl_secs: Some(60),
        }),
        depends_on: vec![],
    };
    let ml_result = StepResult {
        step_id: ml_step.id.clone(),
        success: true,
        output: serde_json::json!({
            "_predictions": [
                {
                    "attribute_name": "segment",
                    "value": "enterprise",
                    "confidence": 0.92
                }
            ]
        }),
        confidence: 0.92,
        started_at,
        completed_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };

    executor
        .record_step_lineage("exec_lineage", &ml_step, &ml_result)
        .await;

    let step_records = tracker.step_records.lock().unwrap();
    assert_eq!(step_records.len(), 1);
    assert_eq!(step_records[0].step_id, "transform_step");
    assert_eq!(step_records[0].modifications.len(), 1);
    drop(step_records);

    let ml_records = tracker.ml_records.lock().unwrap();
    assert_eq!(ml_records.len(), 1);
    assert_eq!(ml_records[0].step_id, "ml_step");
    assert_eq!(ml_records[0].predictions.len(), 1);
    assert_eq!(ml_records[0].predictions[0].attribute_name, "segment");
}

#[tokio::test]
async fn test_record_workflow_lifecycle_lineage_routes_tracker_records() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let tracker = Arc::new(TestLineageTracker::default());
    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone()).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(10);

    executor
        .record_workflow_start_lineage("exec_lifecycle", started_at)
        .await;
    executor
        .record_workflow_completion_lineage("exec_lifecycle", true, completed_at)
        .await;

    let starts = tracker.workflow_starts.lock().unwrap();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].execution_id, "exec_lifecycle");
    assert_eq!(starts[0].started_at, started_at);
    drop(starts);

    let completions = tracker.workflow_completions.lock().unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0], ("exec_lifecycle".to_string(), true));
}

#[tokio::test]
async fn test_initialize_execution_session_records_start_lineage_and_order() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let tracker = Arc::new(TestLineageTracker::default());
    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone()).unwrap();

    let session = executor
        .initialize_execution_session()
        .await
        .expect("execution session should initialize");

    assert!(session.run_state.execution_id.starts_with("exec_"));
    assert_eq!(session.execution_order.len(), 1);
    assert_eq!(session.execution_order[0].id, "gate1");

    let starts = tracker.workflow_starts.lock().unwrap();
    assert_eq!(starts.len(), 1);
    assert_eq!(starts[0].execution_id, session.run_state.execution_id);
    assert_eq!(starts[0].started_at, session.run_state.started_at);
}

#[tokio::test]
async fn test_create_workflow_run_state_builds_prefixed_execution_id() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let before = chrono::Utc::now();
    let run_state = executor.create_workflow_run_state();
    let after = chrono::Utc::now();

    assert!(run_state.execution_id.starts_with("exec_"));
    assert!(run_state.started_at >= before);
    assert!(run_state.started_at <= after);
}

#[tokio::test]
async fn test_compute_execution_order_matches_workflow_dag() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let execution_order = executor
        .compute_execution_order()
        .expect("execution order should resolve from the workflow DAG");

    assert_eq!(execution_order.len(), 1);
    assert_eq!(execution_order[0].id, "gate1");
}

#[tokio::test]
async fn test_initialize_execution_session_without_lineage_tracker_still_builds_session() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let session = executor
        .initialize_execution_session()
        .await
        .expect("execution session should initialize without lineage");

    assert!(session.run_state.execution_id.starts_with("exec_"));
    assert_eq!(session.execution_order.len(), 1);
    assert_eq!(session.execution_order[0].id, "gate1");
}

#[test]
fn test_build_failed_workflow_result_preserves_output_rows_and_error() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let final_output = serde_json::json!({
        "_rows": [
            {"id": 1, "status": "new"},
            {"id": 2, "status": "failed"}
        ],
        "_row_count": 2
    });

    let result = executor.build_failed_workflow_result(
        "exec_fail".to_string(),
        HashMap::new(),
        started_at,
        completed_at,
        0.42,
        "Step 'gate1' failed".to_string(),
        final_output.clone(),
    );

    assert!(!result.success);
    assert_eq!(result.final_decision, FinalDecision::Reject);
    assert_eq!(result.confidence, 0.42);
    assert_eq!(result.error.as_deref(), Some("Step 'gate1' failed"));
    assert_eq!(result.final_output, final_output);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(2));
    assert_eq!(result.output_rows.as_ref().unwrap()[1]["status"], "failed");
}

#[test]
fn test_build_success_workflow_result_preserves_output_rows() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let final_output = serde_json::json!({
        "_rows": [
            {"id": 1, "status": "done"}
        ],
        "_row_count": 1
    });

    let result = executor.build_success_workflow_result(
        "exec_success".to_string(),
        FinalDecision::Accept,
        0.97,
        HashMap::new(),
        started_at,
        completed_at,
        final_output.clone(),
    );

    assert!(result.success);
    assert_eq!(result.final_decision, FinalDecision::Accept);
    assert_eq!(result.confidence, 0.97);
    assert_eq!(result.error, None);
    assert_eq!(result.final_output, final_output);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
    assert_eq!(result.output_rows.as_ref().unwrap()[0]["status"], "done");
}

#[test]
fn test_prepare_step_execution_state_sets_current_row_lineage_step() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "prepare_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let mut context = ExecutionContext::new(serde_json::json!({})).with_row_lineage(
        "exec_lineage".to_string(),
        "job_123".to_string(),
        "tenant_abc".to_string(),
    );

    executor.prepare_step_execution_state(&step, &mut context);

    assert_eq!(
        context
            .row_lineage
            .as_ref()
            .and_then(|lineage| lineage.current_step_id.as_deref()),
        Some("prepare_step")
    );
}

#[tokio::test]
async fn test_compute_final_decision_applies_threshold_and_manual_review_fallback() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step_results = HashMap::from([(
        "gate1".to_string(),
        StepResult {
            step_id: "gate1".to_string(),
            success: true,
            output: serde_json::json!({"confidence": 0.7}),
            confidence: 0.7,
            started_at: chrono::Utc::now(),
            completed_at: chrono::Utc::now(),
            batch_metadata: None,
            runtime_metrics: None,
            batch_frame: None,
        },
    )]);

    let final_decision = executor.compute_final_decision(&step_results).unwrap();

    assert_eq!(final_decision, FinalDecision::ManualReview);
}

#[tokio::test]
async fn test_compute_final_confidence_averages_step_results() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step_results = HashMap::from([
        (
            "gate1".to_string(),
            StepResult {
                step_id: "gate1".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.4}),
                confidence: 0.4,
                started_at: chrono::Utc::now(),
                completed_at: chrono::Utc::now(),
                batch_metadata: None,
                runtime_metrics: None,
                batch_frame: None,
            },
        ),
        (
            "gate2".to_string(),
            StepResult {
                step_id: "gate2".to_string(),
                success: true,
                output: serde_json::json!({"confidence": 0.8}),
                confidence: 0.8,
                started_at: chrono::Utc::now(),
                completed_at: chrono::Utc::now(),
                batch_metadata: None,
                runtime_metrics: None,
                batch_frame: None,
            },
        ),
    ]);

    let final_confidence = executor.compute_final_confidence(&step_results);

    assert!((final_confidence - 0.6).abs() < f64::EPSILON);
}

#[test]
fn test_ensure_step_can_start_rejects_cancelled_context() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::CancellationToken;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "cancelled_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let token = CancellationToken::new();
    token.cancel();
    let context = ExecutionContext::new(serde_json::json!({})).with_cancellation_token(token);

    let error = executor
        .ensure_step_can_start(&step, &context)
        .expect_err("cancelled execution should be rejected before step start");

    assert!(error.to_string().contains("cancelled"));
}

#[test]
fn test_mark_step_execution_progress_updates_tracker() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::progress::ProgressTracker;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "progress_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let tracker = Arc::new(ProgressTracker::new(
        "exec_progress".to_string(),
        "workflow_progress".to_string(),
        1,
    ));
    let context =
        ExecutionContext::new(serde_json::json!({})).with_progress_tracker(tracker.clone());

    executor.mark_step_execution_started(&step, &context);
    let started_snapshot = tracker.snapshot();
    assert_eq!(
        started_snapshot
            .current_step
            .as_ref()
            .map(|current| current.step_name.as_str()),
        Some("progress_step")
    );
    assert_eq!(
        started_snapshot
            .current_step
            .as_ref()
            .map(|current| current.step_type.as_str()),
        Some("ConfidenceGate")
    );

    executor.mark_step_execution_completed(&context);
    let completed_snapshot = tracker.snapshot();
    assert_eq!(completed_snapshot.steps_completed, 1);
}

#[tokio::test]
async fn test_execute_step_rejects_cancelled_context_before_dispatch() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::CancellationToken;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "cancelled_execute_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let token = CancellationToken::new();
    token.cancel();
    let context = ExecutionContext::new(serde_json::json!({ "confidence": 0.9 }))
        .with_cancellation_token(token);

    let error = executor
        .execute_step(&step, &context)
        .await
        .expect_err("cancelled execution should fail before step dispatch");

    assert!(error.to_string().contains("cancelled"));
}

#[tokio::test]
async fn test_execute_step_updates_progress_tracker_for_simple_step() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::progress::ProgressTracker;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let step = WorkflowStep {
        id: "progress_execute_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let tracker = Arc::new(ProgressTracker::new(
        "exec_progress_step".to_string(),
        "workflow_progress_step".to_string(),
        1,
    ));
    let context = ExecutionContext::new(serde_json::json!({ "confidence": 0.9 }))
        .with_progress_tracker(tracker.clone());

    let result = executor
        .execute_step(&step, &context)
        .await
        .expect("simple confidence gate step should execute");

    assert!(result.success);
    let snapshot = tracker.snapshot();
    assert_eq!(snapshot.steps_completed, 1);
    assert_eq!(
        snapshot
            .current_step
            .as_ref()
            .map(|current| current.step_name.as_str()),
        Some("progress_execute_step")
    );
}

#[tokio::test]
async fn test_finalize_step_execution_records_lineage_and_stores_step_result() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::runtime::frame::{BatchFrame, BatchFrameMetadata};

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let tracker = Arc::new(TestLineageTracker::default());
    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone()).unwrap();

    let step = WorkflowStep {
        id: "finalize_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let frame = BatchFrame::from_json_values(&[serde_json::json!({"id": 1, "status": "ok"})])
        .unwrap()
        .with_metadata(BatchFrameMetadata {
            source_step_id: Some("source_step".to_string()),
            source_kind: Some("dataset".to_string()),
            source_id: Some("dataset_123".to_string()),
        });
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: true,
        output: serde_json::json!({
            "_rows": [{"id": 1, "status": "ok"}],
            "_row_count": 1,
            "_modifications": [
                {
                    "field_name": "status",
                    "old_value": "new",
                    "new_value": "ok",
                    "is_reversible": false,
                    "operations": 1
                }
            ]
        }),
        confidence: 0.88,
        started_at: chrono::Utc::now(),
        completed_at: chrono::Utc::now(),
        batch_metadata: Some(frame.metadata().clone()),
        runtime_metrics: None,
        batch_frame: Some(frame),
    };
    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));

    executor
        .finalize_step_execution("exec_finalize", &step, &step_result, &mut state)
        .await
        .unwrap();

    let step_records = tracker.step_records.lock().unwrap();
    assert_eq!(step_records.len(), 1);
    assert_eq!(step_records[0].step_id, "finalize_step");
    assert_eq!(step_records[0].modifications.len(), 1);
    drop(step_records);

    let stored = state
        .step_results
        .get("finalize_step")
        .expect("finalized step result should be stored");
    assert_eq!(stored.output["_row_count"], 1);
    assert!(stored.batch_frame.is_none());
    assert_eq!(
        stored
            .batch_metadata
            .as_ref()
            .and_then(|m| m.source_id.as_deref()),
        Some("dataset_123")
    );
    assert_eq!(
        state
            .context
            .get_batch_frame()
            .unwrap()
            .metadata()
            .source_step_id
            .as_deref(),
        Some("source_step")
    );
}

#[test]
fn test_complete_failed_step_execution_builds_reject_result() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let run_state = WorkflowRunState {
        execution_id: "exec_fail".to_string(),
        started_at,
    };
    let step = WorkflowStep {
        id: "failed_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: false,
        output: serde_json::json!({
            "_rows": [{"id": 1, "status": "failed"}],
            "_row_count": 1
        }),
        confidence: 0.25,
        started_at,
        completed_at: started_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };
    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));
    state
        .step_results
        .insert(step.id.clone(), step_result.clone());
    state.context.working_data = step_result.output.clone();

    let result = executor
        .complete_failed_step_execution(&run_state, &step, &step_result, &state)
        .expect("failed steps should produce a workflow result");

    assert!(!result.success);
    assert_eq!(result.execution_id, run_state.execution_id);
    assert_eq!(result.final_decision, FinalDecision::Reject);
    assert_eq!(result.confidence, 0.25);
    assert_eq!(result.started_at, run_state.started_at);
    assert_eq!(result.error.as_deref(), Some("Step 'failed_step' failed"));
    assert_eq!(result.step_results.len(), 1);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
}

#[test]
fn test_build_failed_workflow_completion_preserves_run_state_and_output_rows() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let run_state = WorkflowRunState {
        execution_id: "exec_failure_build".to_string(),
        started_at,
    };
    let step = WorkflowStep {
        id: "failed_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: false,
        output: serde_json::json!({
            "_rows": [{"id": 1, "status": "failed"}],
            "_row_count": 1
        }),
        confidence: 0.25,
        started_at,
        completed_at: started_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };
    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));
    state
        .step_results
        .insert(step.id.clone(), step_result.clone());
    state.context.working_data = step_result.output.clone();

    let result = executor.build_failed_workflow_completion(
        &run_state,
        &step,
        &step_result,
        &state,
        completed_at,
    );

    assert!(!result.success);
    assert_eq!(result.execution_id, run_state.execution_id);
    assert_eq!(result.started_at, run_state.started_at);
    assert_eq!(result.completed_at, completed_at);
    assert_eq!(result.final_decision, FinalDecision::Reject);
    assert_eq!(result.confidence, 0.25);
    assert_eq!(result.error.as_deref(), Some("Step 'failed_step' failed"));
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
}

#[tokio::test]
async fn test_complete_successful_workflow_execution_records_completion_lineage() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let tracker = Arc::new(TestLineageTracker::default());
    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, tracker.clone()).unwrap();

    let started_at = chrono::Utc::now();
    let run_state = WorkflowRunState {
        execution_id: "exec_success".to_string(),
        started_at,
    };
    let mut step_results = HashMap::new();
    step_results.insert(
        "gate1".to_string(),
        StepResult {
            step_id: "gate1".to_string(),
            success: true,
            output: serde_json::json!({"confidence": 0.95}),
            confidence: 0.95,
            started_at,
            completed_at: started_at,
            batch_metadata: None,
            runtime_metrics: None,
            batch_frame: None,
        },
    );

    let result = executor
        .complete_successful_workflow_execution(
            &run_state,
            step_results,
            serde_json::json!({
                "_rows": [{"id": 1, "status": "done"}],
                "_row_count": 1
            }),
        )
        .await
        .expect("successful execution should finalize cleanly");

    assert!(result.success);
    assert_eq!(result.execution_id, run_state.execution_id);
    assert_eq!(result.final_decision, FinalDecision::Accept);
    assert_eq!(result.confidence, 0.95);
    assert_eq!(result.started_at, run_state.started_at);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));

    let completions = tracker.workflow_completions.lock().unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0], (run_state.execution_id, true));
}

#[test]
fn test_build_successful_workflow_completion_preserves_run_state_and_output_rows() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let run_state = WorkflowRunState {
        execution_id: "exec_success_build".to_string(),
        started_at,
    };
    let mut step_results = HashMap::new();
    step_results.insert(
        "gate1".to_string(),
        StepResult {
            step_id: "gate1".to_string(),
            success: true,
            output: serde_json::json!({"confidence": 0.95}),
            confidence: 0.95,
            started_at,
            completed_at: started_at,
            batch_metadata: None,
            runtime_metrics: None,
            batch_frame: None,
        },
    );
    let final_output = serde_json::json!({
        "_rows": [{"id": 1, "status": "done"}],
        "_row_count": 1
    });

    let result = executor
        .build_successful_workflow_completion(
            &run_state,
            step_results,
            final_output.clone(),
            completed_at,
        )
        .expect("success completion should build a workflow result");

    assert!(result.success);
    assert_eq!(result.execution_id, run_state.execution_id);
    assert_eq!(result.started_at, run_state.started_at);
    assert_eq!(result.completed_at, completed_at);
    assert_eq!(result.final_decision, FinalDecision::Accept);
    assert_eq!(result.confidence, 0.95);
    assert_eq!(result.final_output, final_output);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
}

#[tokio::test]
async fn test_complete_session_outcome_materializes_large_batch_backed_final_output() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::runtime::frame::BatchFrame;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let rows = build_large_result_rows(10_001);
    let frame = BatchFrame::from_json_values(&rows).unwrap();
    let run_state = WorkflowRunState {
        execution_id: "exec_large_final_output".to_string(),
        started_at: chrono::Utc::now(),
    };
    let mut step_results = HashMap::new();
    step_results.insert(
        "gate1".to_string(),
        StepResult {
            step_id: "gate1".to_string(),
            success: true,
            output: serde_json::json!({"confidence": 0.95}),
            confidence: 0.95,
            started_at: run_state.started_at,
            completed_at: run_state.started_at,
            batch_metadata: None,
            runtime_metrics: None,
            batch_frame: None,
        },
    );

    let mut context = ExecutionContext::new(serde_json::json!({
        "_row_count": 10_001,
        "_status": "done"
    }));
    context.batch_frame = Some(frame);

    let result = executor
        .complete_session_outcome(
            &run_state,
            ExecuteLoopOutcome::Completed {
                context,
                step_results,
            },
        )
        .await
        .expect("completion should materialize rows from cached frame");

    assert!(result.success);
    assert_eq!(result.final_output["_row_count"], 10_001);
    assert_eq!(result.final_output["_status"], "done");
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(10_001));
    assert_eq!(result.final_output["_rows"][0]["id"], 0);
    assert_eq!(result.final_output["_rows"][10_000]["id"], 10_000);
}

#[test]
fn test_build_failed_workflow_completion_materializes_large_batch_backed_rows() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;
    use crate::orchestration::workflow::runtime::frame::BatchFrame;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let started_at = chrono::Utc::now();
    let completed_at = started_at + chrono::Duration::milliseconds(5);
    let run_state = WorkflowRunState {
        execution_id: "exec_large_failed_output".to_string(),
        started_at,
    };
    let step = WorkflowStep {
        id: "failed_step".to_string(),
        step_type: StepType::ConfidenceGate,
        config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
            threshold: 0.5,
            input_step: None,
        }),
        depends_on: vec![],
    };
    let step_result = StepResult {
        step_id: step.id.clone(),
        success: false,
        output: serde_json::json!({
            "_row_count": 10_001,
            "_status": "failed"
        }),
        confidence: 0.25,
        started_at,
        completed_at: started_at,
        batch_metadata: None,
        runtime_metrics: None,
        batch_frame: None,
    };

    let rows = build_large_result_rows(10_001);
    let frame = BatchFrame::from_json_values(&rows).unwrap();

    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({})));
    state
        .step_results
        .insert(step.id.clone(), step_result.clone());
    state.context.working_data = serde_json::json!({
        "_row_count": 10_001,
        "_status": "failed"
    });
    state.context.batch_frame = Some(frame);

    let result = executor.build_failed_workflow_completion(
        &run_state,
        &step,
        &step_result,
        &state,
        completed_at,
    );

    assert!(!result.success);
    assert_eq!(result.final_output["_row_count"], 10_001);
    assert_eq!(result.final_output["_status"], "failed");
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(10_001));
    assert_eq!(result.final_output["_rows"][0]["id"], 0);
    assert_eq!(result.final_output["_rows"][10_000]["id"], 10_000);
}

#[tokio::test]
async fn test_execute_ordered_steps_returns_completed_state_for_successful_run() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let execution_order = executor.dag.execution_order().unwrap();
    let run_state = WorkflowRunState {
        execution_id: "exec_loop_success".to_string(),
        started_at: chrono::Utc::now(),
    };

    let outcome = executor
        .execute_ordered_steps(
            &run_state,
            execution_order,
            ExecutionContext::new(serde_json::json!({"confidence": 0.9})),
        )
        .await
        .expect("successful ordered execution should complete");

    match outcome {
        ExecuteLoopOutcome::Completed {
            context,
            step_results,
        } => {
            assert_eq!(step_results.len(), 1);
            assert!(step_results.get("gate1").unwrap().success);
            assert_eq!(context.working_data["confidence"], 0.9);
        }
        ExecuteLoopOutcome::Failed(_) => panic!("expected completed ordered execution"),
    }
}

#[tokio::test]
async fn test_execute_ordered_steps_returns_failed_result_for_failing_step() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let execution_order = executor.dag.execution_order().unwrap();
    let run_state = WorkflowRunState {
        execution_id: "exec_loop_failure".to_string(),
        started_at: chrono::Utc::now(),
    };

    let outcome = executor
        .execute_ordered_steps(
            &run_state,
            execution_order,
            ExecutionContext::new(serde_json::json!({"confidence": 0.1})),
        )
        .await
        .expect("failing ordered execution should still produce a workflow result");

    match outcome {
        ExecuteLoopOutcome::Failed(result) => {
            assert!(!result.success);
            assert_eq!(result.execution_id, run_state.execution_id);
            assert_eq!(result.final_decision, FinalDecision::Reject);
            assert_eq!(result.started_at, run_state.started_at);
            assert_eq!(result.error.as_deref(), Some("Step 'gate1' failed"));
            assert_eq!(result.step_results.len(), 1);
        }
        ExecuteLoopOutcome::Completed { .. } => panic!("expected failed ordered execution"),
    }
}

#[tokio::test]
async fn test_execute_ordered_step_updates_state_for_successful_step() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let step = executor.dag.execution_order().unwrap().remove(0);
    let run_state = WorkflowRunState {
        execution_id: "exec_single_success".to_string(),
        started_at: chrono::Utc::now(),
    };
    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({
        "confidence": 0.9
    })));

    let result = executor
        .execute_ordered_step(&run_state, &step, &mut state)
        .await
        .expect("single ordered step should execute");

    assert!(result.is_none());
    assert_eq!(state.step_results.len(), 1);
    assert!(state.step_results.get("gate1").unwrap().success);
    assert_eq!(state.context.working_data["confidence"], 0.9);
}

#[tokio::test]
async fn test_execute_ordered_step_returns_failed_result_for_rejecting_step() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let step = executor.dag.execution_order().unwrap().remove(0);
    let run_state = WorkflowRunState {
        execution_id: "exec_single_failure".to_string(),
        started_at: chrono::Utc::now(),
    };
    let mut state = ExecuteLoopState::new(ExecutionContext::new(serde_json::json!({
        "confidence": 0.1
    })));

    let result = executor
        .execute_ordered_step(&run_state, &step, &mut state)
        .await
        .expect("single ordered step should return a workflow result");

    let workflow_result = result.expect("rejecting step should fail workflow execution");
    assert!(!workflow_result.success);
    assert_eq!(workflow_result.execution_id, run_state.execution_id);
    assert_eq!(workflow_result.started_at, run_state.started_at);
    assert_eq!(workflow_result.final_decision, FinalDecision::Reject);
    assert_eq!(state.step_results.len(), 1);
}

#[tokio::test]
async fn test_execute_session_returns_failed_result_with_session_run_state() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let session = WorkflowExecutionSession {
        run_state: WorkflowRunState {
            execution_id: "exec_session_failure".to_string(),
            started_at: chrono::Utc::now(),
        },
        execution_order: executor.dag.execution_order().unwrap(),
    };

    let result = executor
        .execute_session(
            session.clone(),
            ExecutionContext::new(serde_json::json!({"confidence": 0.1})),
        )
        .await
        .expect("session execution should return a workflow result");

    assert!(!result.success);
    assert_eq!(result.execution_id, session.run_state.execution_id);
    assert_eq!(result.started_at, session.run_state.started_at);
    assert_eq!(result.final_decision, FinalDecision::Reject);
}

#[tokio::test]
async fn test_complete_session_outcome_returns_failed_result_unchanged() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let session = WorkflowExecutionSession {
        run_state: WorkflowRunState {
            execution_id: "exec_session_outcome_failure".to_string(),
            started_at: chrono::Utc::now(),
        },
        execution_order: vec![],
    };
    let expected = WorkflowResult {
        success: false,
        execution_id: session.run_state.execution_id.clone(),
        final_decision: FinalDecision::Reject,
        confidence: 0.2,
        step_results: HashMap::new(),
        started_at: session.run_state.started_at,
        completed_at: session.run_state.started_at,
        error: Some("failed".to_string()),
        final_output: serde_json::json!({"status": "failed"}),
        output_rows: Some(vec![serde_json::json!({"id": 1})]),
    };

    let result = executor
        .complete_session_outcome(
            &session.run_state,
            ExecuteLoopOutcome::Failed(expected.clone()),
        )
        .await
        .expect("failed session outcome should return the workflow result unchanged");

    assert_eq!(result.success, expected.success);
    assert_eq!(result.execution_id, expected.execution_id);
    assert_eq!(result.final_decision, expected.final_decision);
    assert_eq!(result.confidence, expected.confidence);
    assert_eq!(result.started_at, expected.started_at);
    assert_eq!(result.error, expected.error);
    assert_eq!(result.final_output, expected.final_output);
    assert_eq!(result.output_rows, expected.output_rows);
}

#[tokio::test]
async fn test_complete_session_outcome_finalizes_completed_result() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let session = WorkflowExecutionSession {
        run_state: WorkflowRunState {
            execution_id: "exec_session_outcome_success".to_string(),
            started_at: chrono::Utc::now(),
        },
        execution_order: vec![],
    };
    let mut step_results = HashMap::new();
    step_results.insert(
        "gate1".to_string(),
        StepResult {
            step_id: "gate1".to_string(),
            success: true,
            output: serde_json::json!({"confidence": 0.95}),
            confidence: 0.95,
            started_at: session.run_state.started_at,
            completed_at: session.run_state.started_at,
            batch_metadata: None,
            runtime_metrics: None,
            batch_frame: None,
        },
    );

    let result = executor
        .complete_session_outcome(
            &session.run_state,
            ExecuteLoopOutcome::Completed {
                context: ExecutionContext::new(serde_json::json!({
                    "_rows": [{"id": 1, "status": "done"}],
                    "_row_count": 1
                })),
                step_results,
            },
        )
        .await
        .expect("completed session outcome should finalize successfully");

    assert!(result.success);
    assert_eq!(result.execution_id, session.run_state.execution_id);
    assert_eq!(result.started_at, session.run_state.started_at);
    assert_eq!(result.final_decision, FinalDecision::Accept);
    assert_eq!(result.confidence, 0.95);
    assert_eq!(result.output_rows.as_ref().map(Vec::len), Some(1));
}

#[tokio::test]
async fn test_execute_session_returns_success_result_with_session_run_state() {
    use crate::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};
    use crate::orchestration::rules::RuleExecutor;

    let workflow = create_test_workflow();
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());
    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();
    let session = WorkflowExecutionSession {
        run_state: WorkflowRunState {
            execution_id: "exec_session_success".to_string(),
            started_at: chrono::Utc::now(),
        },
        execution_order: executor.dag.execution_order().unwrap(),
    };

    let result = executor
        .execute_session(
            session.clone(),
            ExecutionContext::new(serde_json::json!({"confidence": 0.9})),
        )
        .await
        .expect("session execution should complete successfully");

    assert!(result.success);
    assert_eq!(result.execution_id, session.run_state.execution_id);
    assert_eq!(result.started_at, session.run_state.started_at);
    assert_eq!(result.final_decision, FinalDecision::Accept);
}
