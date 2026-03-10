//! Simplified Field Transformer Test
//!
//! Basic test to verify FieldTransformer works correctly with sequential steps.

use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelInvoker, ModelRegistry};
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::definition::FallbackStrategy;
use graphica_core::orchestration::workflow::{
    ExecutionContext, FieldTransformation, FieldTransformerConfig, StepConfig, StepType,
    TransformOperation, WorkflowDefinition, WorkflowExecutor, WorkflowStep,
};
use std::sync::Arc;

/// Test: Basic string transformation
#[tokio::test]
async fn test_simple_transformation() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "normalize".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![FieldTransformation {
                    field: "email".to_string(),
                    operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                }],
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

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "  TEST@EXAMPLE.COM  ",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Workflow result: {:#?}", result);

    assert!(result.success);

    let step_result = result.step_results.get("normalize").unwrap();

    // Fields are now directly in output
    assert_eq!(
        step_result.output["email"],
        serde_json::json!("test@example.com"),
        "Email should be trimmed and lowercased"
    );

    // Check metadata
    let mods = step_result.output["_modifications"].as_array().unwrap();
    assert_eq!(mods.len(), 1);

    println!("✅ Simple transformation test passed!");
}

/// Test: Sequential transformations
#[tokio::test]
async fn test_sequential_steps() {
    let workflow = WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "email".to_string(),
                        operations: vec![TransformOperation::Trim],
                    }],
                }),
                depends_on: vec![],
            },
            WorkflowStep {
                id: "step2".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "email".to_string(),
                        operations: vec![TransformOperation::Lower],
                    }],
                }),
                depends_on: vec!["step1".to_string()],
            },
        ],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    let context = ExecutionContext::new(serde_json::json!({
        "email": "  TEST@EXAMPLE.COM  ",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Sequential workflow result: {:#?}", result);

    assert!(result.success);

    // Check step 1 result
    let step1 = result.step_results.get("step1").unwrap();
    assert_eq!(
        step1.output["email"],
        serde_json::json!("TEST@EXAMPLE.COM"),
        "Step 1 should trim whitespace"
    );

    // Check step 2 result
    let step2 = result.step_results.get("step2").unwrap();
    assert_eq!(
        step2.output["email"],
        serde_json::json!("test@example.com"),
        "Step 2 should lowercase (operating on step 1's output)"
    );

    println!("✅ Sequential steps test passed!");
}
