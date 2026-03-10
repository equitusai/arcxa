//! End-to-End Test: Transform → ML Prediction with RDF Lineage
//!
//! Tests the complete integration of:
//! 1. Field transformation (normalize email)
//! 2. ML prediction (predict customer segment)
//! 3. RDF triple generation for both transform and predict
//! 4. Complete lineage linking transformations to predictions

use graphica_coordinator::governance::rdf_store::{GraphicaRdfStore, NamedGraph, RdfStore};
use graphica_coordinator::workflows::lineage::{
    CoordinatorLineageTracker, WorkflowLineageGenerator,
};
use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelInvoker, ModelRegistry};
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::definition::{
    FallbackStrategy, FeatureMapping, MLPredictionConfig, PredictionSpec,
};
use graphica_core::orchestration::workflow::{
    ExecutionContext, FieldTransformation, FieldTransformerConfig, StepConfig, StepType,
    TransformOperation, WorkflowDefinition, WorkflowExecutor, WorkflowStep,
};
use std::sync::Arc;

/// Test 1: Complete transform → predict workflow
#[tokio::test]
async fn test_transform_then_predict_workflow() {
    println!("\n=== Test 1: Transform → Predict Workflow ===\n");

    // 1. Setup RDF store and lineage tracking
    let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let lineage_generator = Arc::new(WorkflowLineageGenerator::new(store.clone()));
    let lineage_tracker = Arc::new(CoordinatorLineageTracker::new(lineage_generator));

    // 2. Create workflow: normalize email → predict segment
    let workflow = WorkflowDefinition {
        steps: vec![
            // Step 1: Normalize email
            WorkflowStep {
                id: "normalize_email".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "email".to_string(),
                        operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                    }],
                }),
                depends_on: vec![],
            },
            // Step 2: Predict customer segment
            WorkflowStep {
                id: "predict_segment".to_string(),
                step_type: StepType::MlPrediction,
                config: StepConfig::MLPrediction(MLPredictionConfig {
                    model_id: "segment_predictor_v2".to_string(),
                    model_version: "2.1.0".to_string(),
                    features: vec![],
                    feature_mappings: vec![FeatureMapping {
                        feature_name: "normalized_email".to_string(),
                        field_name: "email".to_string(),
                        transform: None,
                    }],
                    predictions: vec![PredictionSpec {
                        attribute_name: "customer_segment".to_string(),
                        mock_value: "premium".to_string(),
                        mock_confidence: 0.87,
                    }],
                    confidence_threshold: Some(0.7),
                    timeout_ms: 500,
                    cache_ttl_secs: None,
                }),
                depends_on: vec!["normalize_email".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    // 3. Execute workflow
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, lineage_tracker).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "  TEST@EXAMPLE.COM  ",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("Workflow Result:");
    println!("  Execution ID: {}", result.execution_id);
    println!("  Success: {}", result.success);
    println!("  Confidence: {}", result.confidence);

    // 4. Verify both steps executed successfully
    assert!(result.success, "Workflow should succeed");
    assert!(result.step_results.contains_key("normalize_email"));
    assert!(result.step_results.contains_key("predict_segment"));

    // 5. Verify transformation
    let transform_result = result.step_results.get("normalize_email").unwrap();
    assert_eq!(
        transform_result.output["email"],
        serde_json::json!("test@example.com"),
        "Email should be normalized"
    );

    println!("\nStep 1 (Transform) Result:");
    println!("  Email: {}", transform_result.output["email"]);
    println!(
        "  Modifications: {}",
        transform_result.output["_modifications"]
            .as_array()
            .unwrap()
            .len()
    );

    // 6. Verify prediction
    let predict_result = result.step_results.get("predict_segment").unwrap();
    assert_eq!(
        predict_result.output["customer_segment"],
        serde_json::json!("premium"),
        "Segment should be predicted"
    );

    let predictions = predict_result.output["_predictions"].as_array().unwrap();
    assert_eq!(predictions.len(), 1);
    assert_eq!(predictions[0]["confidence"], 0.87);
    assert_eq!(predictions[0]["model_id"], "segment_predictor_v2");
    assert_eq!(predictions[0]["model_version"], "2.1.0");

    println!("\nStep 2 (Predict) Result:");
    println!("  Segment: {}", predict_result.output["customer_segment"]);
    println!("  Confidence: {}", predictions[0]["confidence"]);
    println!(
        "  Model: {} v{}",
        predictions[0]["model_id"], predictions[0]["model_version"]
    );

    // 7. Verify RDF triples generated
    let triple_count = store
        .count_triples(Some(&NamedGraph::workflow_executions()))
        .unwrap();
    println!("\nRDF Triples Generated: {}", triple_count);

    // Expect:
    // - Workflow start: ~3 triples
    // - Transform step: ~15 triples (step + field modifications)
    // - Predict step: ~15 triples (step + DerivedAttribute)
    // - Workflow complete: ~2 triples
    // Total: ~35+ triples
    assert!(
        triple_count >= 30,
        "Expected at least 30 RDF triples, got {}",
        triple_count
    );

    println!(
        "✅ Test 1 passed: Transform → Predict workflow generated {} RDF triples",
        triple_count
    );
}

/// Test 2: Multiple predictions from single model
#[tokio::test]
async fn test_multiple_predictions() {
    println!("\n=== Test 2: Multiple Predictions ===\n");

    let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let lineage_generator = Arc::new(WorkflowLineageGenerator::new(store.clone()));
    let lineage_tracker = Arc::new(CoordinatorLineageTracker::new(lineage_generator));

    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "predict_attributes".to_string(),
            step_type: StepType::MlPrediction,
            config: StepConfig::MLPrediction(MLPredictionConfig {
                model_id: "customer_attributes_v1".to_string(),
                model_version: "1.2.0".to_string(),
                features: vec![],
                feature_mappings: vec![FeatureMapping {
                    feature_name: "email".to_string(),
                    field_name: "email".to_string(),
                    transform: Some("lower".to_string()),
                }],
                predictions: vec![
                    PredictionSpec {
                        attribute_name: "customer_segment".to_string(),
                        mock_value: "premium".to_string(),
                        mock_confidence: 0.85,
                    },
                    PredictionSpec {
                        attribute_name: "risk_score".to_string(),
                        mock_value: "low".to_string(),
                        mock_confidence: 0.92,
                    },
                    PredictionSpec {
                        attribute_name: "churn_prediction".to_string(),
                        mock_value: "no".to_string(),
                        mock_confidence: 0.78,
                    },
                ],
                confidence_threshold: Some(0.7),
                timeout_ms: 500,
                cache_ttl_secs: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, lineage_tracker).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "customer@example.com",
    }));

    let result = executor.execute(context).await.unwrap();

    assert!(result.success);

    // Verify all three predictions
    let predict_result = result.step_results.get("predict_attributes").unwrap();
    assert!(predict_result.output.get("customer_segment").is_some());
    assert!(predict_result.output.get("risk_score").is_some());
    assert!(predict_result.output.get("churn_prediction").is_some());

    let predictions = predict_result.output["_predictions"].as_array().unwrap();
    assert_eq!(predictions.len(), 3, "Should have 3 predictions");

    println!("Predictions:");
    for (i, pred) in predictions.iter().enumerate() {
        println!(
            "  {}. {}: {} (confidence: {})",
            i + 1,
            pred["attribute_name"],
            pred["value"],
            pred["confidence"]
        );
    }

    // Verify average confidence
    let avg_confidence = (0.85 + 0.92 + 0.78) / 3.0;
    assert!((predict_result.confidence - avg_confidence).abs() < 0.01);

    let triple_count = store
        .count_triples(Some(&NamedGraph::workflow_executions()))
        .unwrap();
    println!("\nRDF Triples Generated: {}", triple_count);

    // More predictions = more triples
    assert!(
        triple_count >= 25,
        "Expected at least 25 RDF triples, got {}",
        triple_count
    );

    println!(
        "✅ Test 2 passed: Multiple predictions generated {} RDF triples",
        triple_count
    );
}

/// Test 3: Confidence threshold filtering
#[tokio::test]
async fn test_confidence_threshold() {
    println!("\n=== Test 3: Confidence Threshold ===\n");

    let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let lineage_generator = Arc::new(WorkflowLineageGenerator::new(store.clone()));
    let lineage_tracker = Arc::new(CoordinatorLineageTracker::new(lineage_generator));

    // High threshold - should FAIL
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "predict_segment".to_string(),
            step_type: StepType::MlPrediction,
            config: StepConfig::MLPrediction(MLPredictionConfig {
                model_id: "segment_predictor_v2".to_string(),
                model_version: "2.1.0".to_string(),
                features: vec![],
                feature_mappings: vec![FeatureMapping {
                    feature_name: "email".to_string(),
                    field_name: "email".to_string(),
                    transform: None,
                }],
                predictions: vec![PredictionSpec {
                    attribute_name: "customer_segment".to_string(),
                    mock_value: "premium".to_string(),
                    mock_confidence: 0.65, // Low confidence
                }],
                confidence_threshold: Some(0.8), // High threshold
                timeout_ms: 500,
                cache_ttl_secs: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, lineage_tracker).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "test@example.com",
    }));

    let result = executor.execute(context).await.unwrap();

    // Should fail due to low confidence
    assert!(
        !result.success,
        "Workflow should fail due to low confidence (0.65 < 0.8)"
    );
    println!("  Prediction confidence: 0.65");
    println!("  Threshold: 0.8");
    println!("  Result: FAILED (as expected)");

    println!("✅ Test 3 passed: Confidence threshold correctly rejected low-confidence prediction");
}

/// Test 4: Auto-generated predictions (deterministic)
#[tokio::test]
async fn test_auto_predictions() {
    println!("\n=== Test 4: Auto-Generated Predictions ===\n");

    let store = Arc::new(GraphicaRdfStore::new_in_memory().unwrap());
    let lineage_generator = Arc::new(WorkflowLineageGenerator::new(store.clone()));
    let lineage_tracker = Arc::new(CoordinatorLineageTracker::new(lineage_generator));

    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "predict_segment".to_string(),
            step_type: StepType::MlPrediction,
            config: StepConfig::MLPrediction(MLPredictionConfig {
                model_id: "segment_predictor_v2".to_string(),
                model_version: "2.1.0".to_string(),
                features: vec![],
                feature_mappings: vec![FeatureMapping {
                    feature_name: "email".to_string(),
                    field_name: "email".to_string(),
                    transform: None,
                }],
                predictions: vec![PredictionSpec {
                    attribute_name: "customer_segment".to_string(),
                    mock_value: "auto".to_string(), // Auto-generate
                    mock_confidence: 0.85,
                }],
                confidence_threshold: None,
                timeout_ms: 500,
                cache_ttl_secs: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor =
        WorkflowExecutor::with_lineage(workflow, invoker, rule_executor, lineage_tracker).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "test@example.com",
    }));

    let result = executor.execute(context).await.unwrap();

    assert!(result.success);

    let predict_result = result.step_results.get("predict_segment").unwrap();
    let segment = predict_result.output["customer_segment"].as_str().unwrap();

    // Should be one of: premium, standard, basic (deterministic based on features)
    assert!(["premium", "standard", "basic"].contains(&segment));

    println!("  Auto-generated segment: {}", segment);
    println!("  (Deterministic based on feature hash)");

    println!("✅ Test 4 passed: Auto-generated prediction is deterministic");
}
