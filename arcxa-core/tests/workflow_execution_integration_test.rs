//! Integration tests for workflow execution
//!
//! Tests the complete workflow execution flow including:
//! - Engine initialization with execution capabilities
//! - Workflow registration
//! - Step execution
//! - Result collection
//! - RDF persistence callback integration

use anyhow::Result;
use graphica_core::orchestration::{
    ml::{CacheConfig, ModelCache, ModelInvoker, ModelRegistry},
    rules::RuleExecutor,
    workflow::{
        definition::{
            ConfidenceGateConfig, FallbackStrategy, StepConfig, StepType, WorkflowDefinition,
            WorkflowStep,
        },
        engine::WorkflowEngine,
        executor::WorkflowResult,
    },
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Helper to create a test ModelInvoker with mock registry
fn create_test_model_invoker() -> Result<Arc<ModelInvoker>> {
    // Create empty registry (no models registered for confidence gate tests)
    let registry = Arc::new(ModelRegistry::new());

    // Create cache with minimal configuration
    let cache_config = CacheConfig {
        max_size: 10,
        default_ttl: std::time::Duration::from_secs(60),
        model_ttls: HashMap::new(),
    };
    let cache = Arc::new(ModelCache::new(cache_config));

    // Create invoker
    let invoker = ModelInvoker::new(registry, cache)?;

    Ok(Arc::new(invoker))
}

/// Helper to create a test RuleExecutor
fn create_test_rule_executor() -> Arc<RuleExecutor> {
    Arc::new(RuleExecutor::new())
}

/// Test fixture for RDF persistence tracking
struct RdfPersistenceTracker {
    persisted_results: Arc<Mutex<Vec<(String, WorkflowResult)>>>,
}

impl RdfPersistenceTracker {
    fn new() -> Self {
        Self {
            persisted_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn create_callback(
        &self,
    ) -> impl Fn(&str, &WorkflowResult) -> Result<()> + Send + Sync + 'static {
        let results = self.persisted_results.clone();
        move |workflow_id: &str, result: &WorkflowResult| {
            results
                .lock()
                .unwrap()
                .push((workflow_id.to_string(), result.clone()));
            Ok(())
        }
    }

    fn get_persisted_count(&self) -> usize {
        self.persisted_results.lock().unwrap().len()
    }

    fn get_last_persisted(&self) -> Option<(String, WorkflowResult)> {
        self.persisted_results.lock().unwrap().last().cloned()
    }
}

#[tokio::test]
async fn test_basic_workflow_execution() -> Result<()> {
    // Create workflow engine with execution capabilities
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();

    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Create a simple workflow with one confidence gate step
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "confidence_check".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    // Register workflow
    let workflow_id = "test_workflow_001";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Test Workflow".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    // Verify workflow is registered
    let registered = engine
        .get_workflow(workflow_id)
        .await?
        .expect("Workflow should be registered");
    assert_eq!(registered.name, "Test Workflow");
    assert_eq!(registered.version, "1.0.0");

    // Execute workflow
    let input = serde_json::json!({
        "entity_id": "test_entity_123",
        "confidence": 0.95
    });
    let context = HashMap::new();

    let result = engine
        .execute_workflow(workflow_id, input, &context)
        .await?;

    // Verify execution result
    assert!(
        !result.execution_id.is_empty(),
        "Execution ID should be generated"
    );
    assert!(result.success, "Execution should succeed");
    assert!(
        result.confidence >= 0.0 && result.confidence <= 1.0,
        "Confidence should be in valid range"
    );
    assert_eq!(result.step_results.len(), 1, "Should have one step result");
    assert!(result.error.is_none(), "Should have no error");

    // Verify step result
    let step_result = result.step_results.get("confidence_check");
    assert!(
        step_result.is_some(),
        "Should have confidence_check step result"
    );

    let step = step_result.unwrap();
    assert_eq!(step.step_id, "confidence_check");
    assert!(step.success, "Step should succeed");
    assert!(
        step.started_at <= step.completed_at,
        "Start time should be before completion"
    );

    Ok(())
}

#[tokio::test]
async fn test_multi_step_workflow_execution() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Create workflow with multiple steps
    let workflow_def = WorkflowDefinition {
        steps: vec![
            WorkflowStep {
                id: "step1".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.7,
                    input_step: None,
                }),
                depends_on: vec![],
            },
            WorkflowStep {
                id: "step2".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.8,
                    input_step: None,
                }),
                depends_on: vec!["step1".to_string()],
            },
            WorkflowStep {
                id: "step3".to_string(),
                step_type: StepType::ConfidenceGate,
                config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                    threshold: 0.9,
                    input_step: None,
                }),
                depends_on: vec!["step2".to_string()],
            },
        ],
        fusion_threshold: 0.85,
        fallback: FallbackStrategy::ManualReview,
    };

    // Register and execute
    let workflow_id = "multi_step_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Multi-Step Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    let input = serde_json::json!({"confidence": 0.95});
    let result = engine
        .execute_workflow(workflow_id, input, &HashMap::new())
        .await?;

    // Verify all steps executed
    assert_eq!(
        result.step_results.len(),
        3,
        "Should have three step results"
    );
    assert!(
        result.step_results.contains_key("step1"),
        "Should have step1"
    );
    assert!(
        result.step_results.contains_key("step2"),
        "Should have step2"
    );
    assert!(
        result.step_results.contains_key("step3"),
        "Should have step3"
    );

    // Verify execution order (step completion times should be sequential)
    let step1_time = result.step_results.get("step1").unwrap().completed_at;
    let step2_time = result.step_results.get("step2").unwrap().completed_at;
    let step3_time = result.step_results.get("step3").unwrap().completed_at;

    assert!(
        step1_time <= step2_time,
        "Step 1 should complete before step 2"
    );
    assert!(
        step2_time <= step3_time,
        "Step 2 should complete before step 3"
    );

    Ok(())
}

#[tokio::test]
async fn test_execution_context_passing() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register simple workflow
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "test_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let workflow_id = "context_test_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Context Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    // Execute with context metadata
    let input = serde_json::json!({
        "test": "data",
        "confidence": 0.9  // Add confidence for ConfidenceGate
    });
    let mut context = HashMap::new();
    context.insert("request_id".to_string(), "req_12345".to_string());
    context.insert("initiator".to_string(), "test_user".to_string());
    context.insert("environment".to_string(), "test".to_string());

    let result = engine
        .execute_workflow(workflow_id, input, &context)
        .await?;

    // Verify execution succeeded with context
    assert!(result.success, "Execution with context should succeed");
    assert!(
        !result.execution_id.is_empty(),
        "Should generate execution ID"
    );

    Ok(())
}

#[tokio::test]
async fn test_rdf_persistence_callback() -> Result<()> {
    // Create tracker and engine with RDF callback
    let tracker = RdfPersistenceTracker::new();
    let callback = tracker.create_callback();

    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor)
        .with_rdf_persistence(callback);

    // Register workflow
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "persistence_test_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let workflow_id = "persistence_test_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Persistence Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    // Execute workflow
    let input = serde_json::json!({"test": "persistence"});
    let result = engine
        .execute_workflow(workflow_id, input, &HashMap::new())
        .await?;

    // Verify RDF persistence callback was invoked
    assert_eq!(
        tracker.get_persisted_count(),
        1,
        "Should have persisted one result"
    );

    let (persisted_workflow_id, persisted_result) = tracker
        .get_last_persisted()
        .expect("Should have persisted result");

    assert_eq!(
        persisted_workflow_id, workflow_id,
        "Workflow ID should match"
    );
    assert_eq!(
        persisted_result.execution_id, result.execution_id,
        "Execution ID should match"
    );
    assert_eq!(
        persisted_result.success, result.success,
        "Success status should match"
    );
    assert_eq!(
        persisted_result.step_results.len(),
        result.step_results.len(),
        "Step count should match"
    );

    Ok(())
}

#[tokio::test]
async fn test_multiple_workflow_executions() -> Result<()> {
    // Create tracker
    let tracker = RdfPersistenceTracker::new();
    let callback = tracker.create_callback();

    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor)
        .with_rdf_persistence(callback);

    // Register workflow
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "test_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let workflow_id = "multi_execution_test";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Multi Execution Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    // Execute multiple times
    let num_executions = 5;
    let mut execution_ids = Vec::new();

    for i in 0..num_executions {
        let input = serde_json::json!({
            "iteration": i,
            "confidence": 0.9
        });
        let result = engine
            .execute_workflow(workflow_id, input, &HashMap::new())
            .await?;

        assert!(result.success, "Execution {} should succeed", i);
        execution_ids.push(result.execution_id.clone());
    }

    // Verify all executions had unique IDs
    let unique_ids: std::collections::HashSet<_> = execution_ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        num_executions,
        "All execution IDs should be unique"
    );

    // Verify all executions were persisted
    assert_eq!(
        tracker.get_persisted_count(),
        num_executions,
        "Should have persisted all executions"
    );

    Ok(())
}

#[tokio::test]
async fn test_workflow_execution_stats() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register workflow
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "stats_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let workflow_id = "stats_test_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Stats Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await?;

    // Execute and get metadata
    let input = serde_json::json!({"test": "stats"});
    let _result = engine
        .execute_workflow(workflow_id, input, &HashMap::new())
        .await?;

    // Get workflow metadata to check execution stats
    let metadata = engine
        .get_workflow(workflow_id)
        .await?
        .expect("Workflow should exist");

    // Verify stats
    assert_eq!(metadata.execution_count, 1, "Should have one execution");
    assert!(
        metadata.last_executed_at.is_some(),
        "Should have last execution time"
    );

    Ok(())
}

#[tokio::test]
async fn test_engine_without_execution_capabilities() {
    // Create engine WITHOUT execution capabilities (just registration)
    let engine = WorkflowEngine::new();

    // Register workflow
    let workflow_def = WorkflowDefinition {
        steps: vec![WorkflowStep {
            id: "test_step".to_string(),
            step_type: StepType::ConfidenceGate,
            config: StepConfig::ConfidenceGate(ConfidenceGateConfig {
                threshold: 0.8,
                input_step: None,
            }),
            depends_on: vec![],
        }],
        fusion_threshold: 0.8,
        fallback: FallbackStrategy::ManualReview,
    };

    let workflow_id = "no_exec_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "No Exec Test".to_string(),
            workflow_def,
            None,
            vec![],
        )
        .await
        .expect("Registration should succeed");

    // Try to execute - should fail with clear error
    let input = serde_json::json!({"test": "data"});
    let result = engine
        .execute_workflow(workflow_id, input, &HashMap::new())
        .await;

    assert!(
        result.is_err(),
        "Execution should fail without capabilities"
    );
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("not configured for execution"),
        "Error should indicate missing execution configuration"
    );
}
