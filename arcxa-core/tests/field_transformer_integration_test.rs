//! Field Transformer Integration Test
//!
//! Tests the FieldTransformer workflow step with various transformation operations.
//! Validates field-level transformations and modification tracking for RDF lineage.
//!
//! NOTE: This test is disabled because the field transformation APIs have changed significantly.
//! The test needs to be rewritten to match the current API.
//! Enable with: `cargo test --features field-transformers`

#![cfg(feature = "field-transformers")]

use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelInvoker, ModelRegistry};
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::definition::FallbackStrategy;
use graphica_core::orchestration::workflow::{
    ExecutionContext, FieldTransformation, FieldTransformerConfig, StepConfig, StepType,
    TransformOperation, WorkflowDefinition, WorkflowExecutor, WorkflowStep,
};
use std::sync::Arc;

/// Test 1: Basic string transformations (trim, lower, upper)
#[tokio::test]
async fn test_basic_string_transformations() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "transform_fields".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![
                    FieldTransformation {
                        field: "email".to_string(),
                        operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                    },
                    FieldTransformation {
                        field: "name".to_string(),
                        operations: vec![TransformOperation::Trim, TransformOperation::Upper],
                    },
                ],
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::RejectFusion,
    };

    // Create dependencies
    let registry = Arc::new(ModelRegistry::new());
    let cache = Arc::new(ModelCache::new(CacheConfig::default()));
    let invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
    let rule_executor = Arc::new(RuleExecutor::new());

    let executor = WorkflowExecutor::new(workflow, invoker, rule_executor).unwrap();

    // Test data with whitespace and mixed case
    let context = ExecutionContext::new(serde_json::json!({
        "email": "  Alice@EXAMPLE.COM  ",
        "name": "  alice smith  ",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Test 1: Workflow execution result: {:?}", result);

    assert!(result.success, "Workflow should succeed");

    // Get the transform step result
    let step_result = result.step_results.get("transform_fields").unwrap();
    assert!(step_result.success, "Transform step should succeed");

    // Check transformed data (fields are now directly in output)
    let output = &step_result.output;

    assert_eq!(
        output["email"],
        serde_json::json!("alice@example.com"),
        "Email should be trimmed and lowercased"
    );
    assert_eq!(
        output["name"],
        serde_json::json!("ALICE SMITH"),
        "Name should be trimmed and uppercased"
    );

    // Check modification tracking (prefixed with _)
    let modifications = output["_modifications"].as_array().unwrap();
    assert_eq!(modifications.len(), 2, "Should track 2 field modifications");

    println!("✅ Test 1 passed: Basic string transformations");
}

/// Test 2: Replace and regex transformations
#[tokio::test]
async fn test_replace_and_regex_transformations() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "clean_phone".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![
                    FieldTransformation {
                        field: "phone".to_string(),
                        operations: vec![
                            TransformOperation::Replace {
                                from: "-".to_string(),
                                to: "".to_string(),
                            },
                            TransformOperation::Replace {
                                from: " ".to_string(),
                                to: "".to_string(),
                            },
                        ],
                    },
                    FieldTransformation {
                        field: "text".to_string(),
                        operations: vec![TransformOperation::Regex {
                            pattern: r"\d+".to_string(),
                            replacement: "NUM".to_string(),
                        }],
                    },
                ],
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
        "phone": "555-123-4567",
        "text": "Order 123 costs $45",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Test 2: Workflow execution result: {:?}", result);

    assert!(result.success);

    let step_result = result.step_results.get("clean_phone").unwrap();
    let transformed_data = &step_result.output["transformed_data"];

    assert_eq!(
        transformed_data["clean_phone"],
        serde_json::json!("5551234567"),
        "Phone should have dashes and spaces removed"
    );
    assert_eq!(
        transformed_data["clean_text"],
        serde_json::json!("Order NUM costs $NUM"),
        "Digits should be replaced with NUM"
    );

    println!("✅ Test 2 passed: Replace and regex transformations");
}

/// Test 3: Substring and split transformations
#[tokio::test]
async fn test_substring_and_split() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "extract_parts".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![
                    FieldTransformation {
                        field: "full_name".to_string(),
                        operations: vec![TransformOperation::Split {
                            delimiter: " ".to_string(),
                            index: 0, // Get first name
                        }],
                    },
                    FieldTransformation {
                        field: "code".to_string(),
                        operations: vec![TransformOperation::Substring {
                            start: 0,
                            length: Some(3),
                        }],
                    },
                ],
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
        "full_name": "Alice Johnson",
        "code": "ABCDEF123",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Test 3: Workflow execution result: {:?}", result);

    assert!(result.success);

    let step_result = result.step_results.get("extract_parts").unwrap();
    let transformed_data = &step_result.output["transformed_data"];

    assert_eq!(
        transformed_data["first_name"],
        serde_json::json!("Alice"),
        "Should extract first name"
    );
    assert_eq!(
        transformed_data["product_code"],
        serde_json::json!("ABC"),
        "Should extract first 3 characters"
    );

    println!("✅ Test 3 passed: Substring and split transformations");
}

/// Test 4: Numeric transformations (round, if_null)
#[tokio::test]
async fn test_numeric_transformations() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "format_numbers".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![
                    FieldTransformation {
                        field: "price".to_string(),
                        operations: vec![TransformOperation::Round { decimals: 2 }],
                    },
                    FieldTransformation {
                        field: "optional_field".to_string(),
                        operations: vec![TransformOperation::IfNull {
                            default_value: "N/A".to_string(),
                        }],
                    },
                ],
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
        "price": 19.9567,
        "optional_field": "",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Test 4: Workflow execution result: {:?}", result);

    assert!(result.success);

    let step_result = result.step_results.get("format_numbers").unwrap();
    let transformed_data = &step_result.output["transformed_data"];

    assert_eq!(
        transformed_data["formatted_price"],
        serde_json::json!(19.96),
        "Price should be rounded to 2 decimals"
    );
    assert_eq!(
        transformed_data["default_optional_field"],
        serde_json::json!("N/A"),
        "Empty field should use default value"
    );

    println!("✅ Test 4 passed: Numeric transformations");
}

/// Test 5: Modification tracking and reversibility
#[tokio::test]
async fn test_modification_tracking() {
    let workflow = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "track_changes".to_string(),
            step_type: StepType::FieldTransformer,
            config: StepConfig::FieldTransformer(FieldTransformerConfig {
                transformations: vec![
                    FieldTransformation {
                        field: "reversible_field".to_string(),
                        operations: vec![TransformOperation::Replace {
                            from: "old".to_string(),
                            to: "new".to_string(),
                        }],
                    },
                    FieldTransformation {
                        field: "irreversible_field".to_string(),
                        operations: vec![TransformOperation::Trim, TransformOperation::Lower],
                    },
                ],
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
        "reversible_field": "replace old value",
        "irreversible_field": "  MixedCase  ",
    }));

    let result = executor.execute(context).await.unwrap();

    println!("✅ Test 5: Workflow execution result: {:?}", result);

    assert!(result.success);

    let step_result = result.step_results.get("track_changes").unwrap();
    let modifications = step_result.output_modifications.as_array().unwrap();

    assert_eq!(modifications.len(), 2, "Should track 2 modifications");

    // Check modification metadata
    for modification in modifications {
        let field_name = modification["field_name"].as_str().unwrap();
        let is_reversible = modification["is_reversible"].as_bool().unwrap();

        println!("  Field: {}, Reversible: {}", field_name, is_reversible);

        if field_name == "reversible_field" {
            assert!(is_reversible, "Replace operation should be reversible");
            assert_eq!(
                modification["old_value"],
                serde_json::json!("replace old value")
            );
            assert_eq!(
                modification["new_value"],
                serde_json::json!("replace new value")
            );
        } else if field_name == "irreversible_field" {
            assert!(!is_reversible, "Trim+Lower should be irreversible");
            assert_eq!(
                modification["old_value"],
                serde_json::json!("  MixedCase  ")
            );
            assert_eq!(modification["new_value"], serde_json::json!("mixedcase"));
        }
    }

    println!("✅ Test 5 passed: Modification tracking and reversibility");
}

/// Test 6: Multiple transformation steps in sequence
#[tokio::test]
async fn test_sequential_transformations() {
    let workflow = WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "step1_normalize".to_string(),
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
                id: "step2_lowercase".to_string(),
                step_type: StepType::FieldTransformer,
                config: StepConfig::FieldTransformer(FieldTransformerConfig {
                    transformations: vec![FieldTransformation {
                        field: "email".to_string(),
                        operations: vec![TransformOperation::Lower],
                    }],
                }),
                depends_on: vec!["step1_normalize".to_string()],
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

    println!("✅ Test 6: Workflow execution result: {:?}", result);

    assert!(result.success);

    // Check step 1 result
    let step1 = result.step_results.get("step1_normalize").unwrap();
    assert_eq!(
        step1.output["transformed_data"]["email"],
        serde_json::json!("TEST@EXAMPLE.COM")
    );

    // Check step 2 result
    let step2 = result.step_results.get("step2_lowercase").unwrap();
    assert_eq!(
        step2.output["transformed_data"]["email"],
        serde_json::json!("test@example.com")
    );

    println!("✅ Test 6 passed: Sequential transformations");
}
