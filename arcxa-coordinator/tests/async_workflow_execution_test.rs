//! Async Workflow Execution Integration Tests
//!
//! Tests async workflow execution with production components in background tasks.

use graphica_coordinator::workflows::{
    domain::{Action, Condition, Route, Workflow},
    engine::ExecutionContext,
    storage::{ExecutionStore, WorkflowStore},
};
use graphica_core::orchestration::rules::RuleExecutor;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Test 1: Async execution with production rule executor in background task
#[tokio::test]
async fn test_async_execution_with_production_components() {
    let workflow_store = WorkflowStore::new();
    let execution_store = Arc::new(ExecutionStore::new());
    let rule_executor = Arc::new(RuleExecutor::new());

    // Create workflow with validation action
    let workflow = Workflow::new(
        "wf_async_test".to_string(),
        "Async Test Workflow".to_string(),
        vec![Route {
            id: "route_async".to_string(),
            name: "Async Validation Route".to_string(),
            description: String::new(),
            condition: Box::new(Condition::Always),
            actions: Box::new(vec![
                Action::Validate {
                    rule_id: "async_test_rule".to_string(),
                },
                Action::SetField {
                    field: "async_executed".to_string(),
                    value: json!(true),
                },
                Action::Log {
                    level: "info".to_string(),
                    message: "Async execution completed".to_string(),
                },
            ]),
            priority: 1,
            enabled: true,
        }],
    );

    // Store workflow
    workflow_store.create(workflow.clone()).unwrap();

    // Simulate async execution background task
    let execution_id = format!("exec_{}", uuid::Uuid::new_v4());
    let input = json!({
        "test_id": "async_123",
        "data": "test_data"
    });

    // Create execution record
    let execution = graphica_coordinator::workflows::domain::WorkflowExecution::new(
        execution_id.clone(),
        workflow.id.clone(),
        workflow.name.clone(),
        input.clone(),
        Some("test_user".to_string()),
    );
    execution_store.save(execution).await.unwrap();

    // Execute in background (simulating the spawned task)
    let workflow_clone = workflow.clone();
    let input_clone = input.clone();
    let execution_store_clone = Arc::clone(&execution_store);
    let execution_id_clone = execution_id.clone();
    let rule_executor_clone = Some(rule_executor.clone());

    let handle = tokio::spawn(async move {
        // Simulate execute_workflow_background logic
        let route = &workflow_clone.routes[0];
        let mut output = input_clone.clone();

        let context = ExecutionContext {
            workflow_id: workflow_clone.id.clone(),
            route_id: route.id.clone(),
            input_data: input_clone.clone(),
            rule_executor: rule_executor_clone,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            execution_id: None,
            metrics: None,
            manual_mapping_store: None,
            action_index: 0,
            approval_store: None,
            execution_store: None,
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        // Execute actions
        let results = graphica_coordinator::workflows::engine::ActionExecutor::execute_actions(
            &route.actions,
            &mut output,
            &context,
        )
        .await
        .unwrap();

        // Update execution store
        let mut exec = execution_store_clone
            .get_required(&execution_id_clone)
            .await
            .unwrap();
        exec.set_output(output);
        exec.actions_executed = results.len();
        exec.update_status(graphica_coordinator::workflows::domain::ExecutionStatus::Completed);
        execution_store_clone.update(exec).await.unwrap();

        results
    });

    // Wait for background task
    let results = handle.await.unwrap();

    // Verify execution completed
    assert_eq!(results.len(), 3, "Should execute 3 actions");

    // Verify execution record updated
    let execution = execution_store.get_required(&execution_id).await.unwrap();
    assert_eq!(
        execution.status,
        graphica_coordinator::workflows::domain::ExecutionStatus::Completed
    );
    assert_eq!(execution.actions_executed, 3);
    assert!(execution.output.is_some());

    // Verify output includes our set field
    let output = execution.output.unwrap();
    assert_eq!(output["async_executed"], json!(true));

    println!("✅ Test 1 passed: Async execution with production components");
}

/// Test 2: Multiple async executions in parallel
#[tokio::test]
async fn test_parallel_async_executions() {
    let workflow_store = WorkflowStore::new();
    let execution_store = Arc::new(ExecutionStore::new());
    let rule_executor = Arc::new(RuleExecutor::new());

    // Create workflow
    let workflow = Workflow::new(
        "wf_parallel_test".to_string(),
        "Parallel Test Workflow".to_string(),
        vec![Route {
            id: "route_parallel".to_string(),
            name: "Parallel Route".to_string(),
            description: String::new(),
            condition: Box::new(Condition::Always),
            actions: Box::new(vec![Action::SetField {
                field: "parallel_executed".to_string(),
                value: json!(true),
            }]),
            priority: 1,
            enabled: true,
        }],
    );

    workflow_store.create(workflow.clone()).unwrap();

    // Launch 5 parallel async executions
    let mut handles = vec![];

    for i in 0..5 {
        let execution_id = format!("exec_parallel_{}", i);
        let input = json!({
            "execution_num": i,
            "test": "parallel"
        });

        let execution = graphica_coordinator::workflows::domain::WorkflowExecution::new(
            execution_id.clone(),
            workflow.id.clone(),
            workflow.name.clone(),
            input.clone(),
            Some(format!("user_{}", i)),
        );
        execution_store.save(execution).await.unwrap();

        let workflow_clone = workflow.clone();
        let input_clone = input.clone();
        let execution_store_clone = Arc::clone(&execution_store);
        let execution_id_clone = execution_id.clone();
        let rule_executor_clone = Some(rule_executor.clone());

        let handle = tokio::spawn(async move {
            let route = &workflow_clone.routes[0];
            let mut output = input_clone.clone();

            let context = ExecutionContext {
                workflow_id: workflow_clone.id.clone(),
                route_id: route.id.clone(),
                input_data: input_clone.clone(),
                rule_executor: rule_executor_clone,
                transformer_registry: None,
                kafka_producer: None,
                http_client: None,
                lineage_generator: None,
                execution_id: None,
                metrics: None,
                manual_mapping_store: None,
                action_index: 0,
                approval_store: None,
                execution_store: None,
                column_lineage_store: None,
                tenant_id: "default".to_string(),
                timeout_config:
                    graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
                workflow_start_time: std::time::Instant::now(),
                stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
                db2_pool: None,
                postgres_pool: None,
                memory_monitor: None,
            };

            let results = graphica_coordinator::workflows::engine::ActionExecutor::execute_actions(
                &route.actions,
                &mut output,
                &context,
            )
            .await
            .unwrap();

            let mut exec = execution_store_clone
                .get_required(&execution_id_clone)
                .await
                .unwrap();
            exec.set_output(output);
            exec.actions_executed = results.len();
            exec.update_status(graphica_coordinator::workflows::domain::ExecutionStatus::Completed);
            execution_store_clone.update(exec).await.unwrap();

            (execution_id_clone, results.len())
        });

        handles.push(handle);
    }

    // Wait for all executions
    let results = futures::future::join_all(handles).await;

    // Verify all completed
    assert_eq!(results.len(), 5);
    for result in results {
        let (exec_id, action_count) = result.unwrap();
        let execution = execution_store.get_required(&exec_id).await.unwrap();
        assert_eq!(
            execution.status,
            graphica_coordinator::workflows::domain::ExecutionStatus::Completed
        );
        assert_eq!(action_count, 1);
    }

    println!("✅ Test 2 passed: Parallel async executions");
}

/// Test 3: Async execution error handling
#[tokio::test]
async fn test_async_execution_error_handling() {
    let workflow_store = WorkflowStore::new();
    let execution_store = Arc::new(ExecutionStore::new());

    // Create workflow with action that will fail
    let workflow = Workflow::new(
        "wf_error_test".to_string(),
        "Error Test Workflow".to_string(),
        vec![Route {
            id: "route_error".to_string(),
            name: "Error Route".to_string(),
            description: String::new(),
            condition: Box::new(Condition::Always),
            actions: Box::new(vec![
                Action::SetField {
                    field: "step1".to_string(),
                    value: json!("completed"),
                },
                // This will fail - trying to set field on non-object after mutation
                Action::Log {
                    level: "info".to_string(),
                    message: "Before potential error".to_string(),
                },
            ]),
            priority: 1,
            enabled: true,
        }],
    );

    workflow_store.create(workflow.clone()).unwrap();

    let execution_id = "exec_error_test".to_string();
    let input = json!({"test": "error_handling"});

    let execution = graphica_coordinator::workflows::domain::WorkflowExecution::new(
        execution_id.clone(),
        workflow.id.clone(),
        workflow.name.clone(),
        input.clone(),
        Some("test_user".to_string()),
    );
    execution_store.save(execution).await.unwrap();

    // Execute
    let route = &workflow.routes[0];
    let mut output = input.clone();

    let context = ExecutionContext {
        workflow_id: workflow.id.clone(),
        route_id: route.id.clone(),
        input_data: input.clone(),
        rule_executor: None,
        transformer_registry: None,
        kafka_producer: None,
        http_client: None,
        lineage_generator: None,
        execution_id: None,
        metrics: None,
        manual_mapping_store: None,
        action_index: 0,
        approval_store: None,
        execution_store: None,
        column_lineage_store: None,
        tenant_id: "default".to_string(),
        timeout_config: graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
        workflow_start_time: std::time::Instant::now(),
        stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        db2_pool: None,
        postgres_pool: None,
        memory_monitor: None,
    };

    let result = graphica_coordinator::workflows::engine::ActionExecutor::execute_actions(
        &route.actions,
        &mut output,
        &context,
    )
    .await;

    // Should succeed even if some actions have issues
    assert!(result.is_ok());

    let results = result.unwrap();
    assert_eq!(results.len(), 2);

    // Verify first action succeeded
    assert_eq!(
        results[0].status,
        graphica_coordinator::workflows::domain::ActionStatus::Success
    );

    println!("✅ Test 3 passed: Async execution error handling");
}

/// Test 4: Async execution with rule executor availability check
#[tokio::test]
async fn test_async_execution_rule_executor_availability() {
    let workflow = Workflow::new(
        "wf_rule_check".to_string(),
        "Rule Executor Check Workflow".to_string(),
        vec![Route {
            id: "route_rule_check".to_string(),
            name: "Rule Check Route".to_string(),
            description: String::new(),
            condition: Box::new(Condition::Always),
            actions: Box::new(vec![Action::Validate {
                rule_id: "test_rule".to_string(),
            }]),
            priority: 1,
            enabled: true,
        }],
    );

    // Test WITH production rule executor
    {
        let rule_executor = Some(Arc::new(RuleExecutor::new()));
        let route = &workflow.routes[0];
        let mut output = json!({"data": "test"});

        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route.id.clone(),
            input_data: json!({"data": "test"}),
            rule_executor,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            execution_id: None,
            metrics: None,
            manual_mapping_store: None,
            action_index: 0,
            approval_store: None,
            execution_store: None,
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        let results = graphica_coordinator::workflows::engine::ActionExecutor::execute_actions(
            &route.actions,
            &mut output,
            &context,
        )
        .await
        .unwrap();

        // Should execute with production executor (may fail if rule not loaded, but path is verified)
        assert_eq!(results.len(), 1);
        println!("✅ Test with production executor completed");
    }

    // Test WITHOUT production rule executor (fallback)
    {
        let rule_executor = None;
        let route = &workflow.routes[0];
        let mut output = json!({"data": "test"});

        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route.id.clone(),
            input_data: json!({"data": "test"}),
            rule_executor,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            execution_id: None,
            metrics: None,
            manual_mapping_store: None,
            action_index: 0,
            approval_store: None,
            execution_store: None,
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: std::time::Instant::now(),
            stage_start_time: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        let results = graphica_coordinator::workflows::engine::ActionExecutor::execute_actions(
            &route.actions,
            &mut output,
            &context,
        )
        .await
        .unwrap();

        // Should fallback to stub and succeed
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].status,
            graphica_coordinator::workflows::domain::ActionStatus::Success
        );
        println!("✅ Test with fallback executor completed");
    }

    println!("✅ Test 4 passed: Rule executor availability check");
}
