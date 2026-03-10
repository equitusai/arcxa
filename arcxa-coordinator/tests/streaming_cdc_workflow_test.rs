//! Streaming CDC Workflow Integration Test
//!
//! Tests the production streaming pipeline:
//! CDC Events → Debezium Parser → Workflow Router → Action Execution → State Persistence
//!
//! This test verifies Phase 4 functionality:
//! - CDC event parsing (Debezium format)
//! - Workflow routing based on CDC data
//! - Action execution in streaming context
//! - Execution state tracking

use graphica_coordinator::workflows::domain::{
    Action, Condition, DebeziumEvent, Route, Workflow, WorkflowExecution,
};
use graphica_coordinator::workflows::engine::{ActionExecutor, ExecutionContext, WorkflowRouter};
use graphica_coordinator::workflows::storage::{ExecutionStore, WorkflowStore};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn test_cdc_event_parsing_and_routing() {
    // Create a CDC event (Debezium format - PostgreSQL connector)
    let cdc_json = json!({
        "before": null,
        "after": {
            "id": 123,
            "customer_name": "Alice Johnson",
            "email": "alice@example.com",
            "customer_type": "premium"
        },
        "op": "c", // Create operation
        "source": {
            "version": "2.1.0",
            "connector": "postgresql",
            "db": "ecommerce",
            "schema": "public",
            "table": "customers",
            "txId": 567890,
            "lsn": 98765432,
            "ts_ms": 1698765432000_i64
        },
        "ts_ms": 1698765432000_i64
    });

    // Parse CDC event
    let cdc_event = DebeziumEvent::from_json_value(&cdc_json).expect("Failed to parse CDC event");

    // Verify CDC parsing
    assert_eq!(cdc_event.source.table, "customers");
    assert_eq!(cdc_event.source.database, Some("ecommerce".to_string()));
    assert!(cdc_event.after.is_some());
    assert!(cdc_event.before.is_none());

    // Convert to workflow input
    let workflow_input = cdc_event.to_workflow_input();

    // Verify workflow input structure
    assert_eq!(workflow_input["operation"], "create");
    assert_eq!(workflow_input["customer_name"], "Alice Johnson");
    assert_eq!(workflow_input["email"], "alice@example.com");
    assert_eq!(workflow_input["customer_type"], "premium");
    assert_eq!(workflow_input["source"]["table"], "customers");
    assert_eq!(workflow_input["source"]["database"], "ecommerce");

    // Create workflow with routes for different customer types
    let routes = vec![
        Route::with_priority(
            "premium_route",
            "Premium Customer Processing",
            Condition::equals("customer_type", "premium"),
            vec![
                Action::Log {
                    level: "info".to_string(),
                    message: "Processing premium customer".to_string(),
                },
                Action::SetField {
                    field: "priority".to_string(),
                    value: json!("high"),
                },
            ],
            100, // High priority
        ),
        Route::with_priority(
            "standard_route",
            "Standard Customer Processing",
            Condition::equals("customer_type", "standard"),
            vec![Action::Log {
                level: "info".to_string(),
                message: "Processing standard customer".to_string(),
            }],
            50, // Medium priority
        ),
    ];

    let workflow = Workflow::new("customer_cdc_workflow", "Customer CDC Processing", routes);

    // Route the CDC event through the workflow
    let route_match =
        WorkflowRouter::select_route(&workflow, &workflow_input).expect("Routing failed");

    // Verify correct route was selected
    assert!(route_match.is_some());
    let route_match = route_match.unwrap();
    assert_eq!(route_match.route.id, "premium_route");
    assert_eq!(route_match.route.actions.len(), 2);

    println!(
        "✅ CDC event successfully parsed and routed to: {}",
        route_match.route.name
    );
}

#[tokio::test]
async fn test_streaming_workflow_execution() {
    // Create workflow store
    let workflow_store = Arc::new(WorkflowStore::new());

    // Create execution store for tracking
    let execution_store = Arc::new(ExecutionStore::new());

    // Define workflow with multiple routes
    let routes = vec![
        Route::with_priority(
            "route_insert",
            "Handle Inserts",
            Condition::equals("operation", "create"),
            vec![
                Action::Log {
                    level: "info".to_string(),
                    message: "New record inserted".to_string(),
                },
                Action::SetField {
                    field: "processed".to_string(),
                    value: json!(true),
                },
            ],
            100,
        ),
        Route::with_priority(
            "route_update",
            "Handle Updates",
            Condition::equals("operation", "update"),
            vec![Action::Log {
                level: "info".to_string(),
                message: "Record updated".to_string(),
            }],
            90,
        ),
        Route::with_priority(
            "route_delete",
            "Handle Deletes",
            Condition::equals("operation", "delete"),
            vec![Action::Log {
                level: "warn".to_string(),
                message: "Record deleted".to_string(),
            }],
            80,
        ),
    ];

    let workflow = Workflow::new("cdc_streaming_workflow", "CDC Stream Processor", routes);
    let workflow_id = workflow.id.clone();

    // Save workflow
    workflow_store
        .create(workflow.clone())
        .expect("Failed to save workflow");

    // Simulate CDC events
    let test_events = vec![
        // Event 1: Create
        json!({
            "operation": "create",
            "id": 1,
            "name": "Customer A",
            "source": {"table": "customers", "database": "test", "schema": "public"}
        }),
        // Event 2: Update
        json!({
            "operation": "update",
            "id": 1,
            "name": "Customer A Updated",
            "source": {"table": "customers", "database": "test", "schema": "public"},
            "changed_fields": ["name"]
        }),
        // Event 3: Delete
        json!({
            "operation": "delete",
            "id": 1,
            "source": {"table": "customers", "database": "test", "schema": "public"}
        }),
    ];

    // Process each event through the workflow
    for (idx, event_data) in test_events.iter().enumerate() {
        // Route event
        let route_match =
            WorkflowRouter::select_route(&workflow, event_data).expect("Failed to route event");

        assert!(route_match.is_some(), "No route matched for event {}", idx);
        let route_match = route_match.unwrap();

        // Create execution record
        let execution_id = format!("exec_stream_{}", idx);
        let execution = WorkflowExecution::new(
            execution_id.clone(),
            workflow_id.clone(),
            workflow.name.clone(),
            event_data.clone(),
            Some("streaming_executor".to_string()),
        );

        execution_store
            .save(execution.clone())
            .await
            .expect("Failed to save execution");

        // Create execution context (minimal - no production integrations for test)
        let context = ExecutionContext {
            workflow_id: workflow_id.clone(),
            route_id: route_match.route.id.clone(),
            input_data: event_data.clone(),
            rule_executor: None,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            execution_id: Some(execution_id.clone()),
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
        let mut data = event_data.clone();
        let results =
            ActionExecutor::execute_actions(&route_match.route.actions, &mut data, &context)
                .await
                .expect("Action execution failed");

        // Verify execution results
        assert!(!results.is_empty(), "No actions executed for event {}", idx);

        // Verify execution state was tracked
        let stored_execution = execution_store
            .get(&execution_id)
            .await
            .expect("Failed to retrieve execution");
        assert!(stored_execution.is_some(), "Execution not found in store");

        println!(
            "✅ Event {} processed via route '{}' with {} actions",
            idx,
            route_match.route.name,
            results.len()
        );
    }

    println!("✅ All streaming events processed successfully");
}

#[tokio::test]
async fn test_cdc_update_with_changed_fields() {
    // Create a CDC update event with before/after states
    let cdc_json = json!({
        "before": {
            "id": 456,
            "name": "Bob Smith",
            "email": "bob@example.com",
            "status": "active",
            "credit_limit": 1000
        },
        "after": {
            "id": 456,
            "name": "Bob Smith",
            "email": "bob.smith@example.com",  // Email changed
            "status": "active",
            "credit_limit": 2000  // Credit limit increased
        },
        "op": "u", // Update operation
        "source": {
            "connector": "postgresql",
            "db": "crm",
            "schema": "public",
            "table": "accounts",
            "txId": 789,
            "lsn": 123456
        },
        "ts_ms": 1698765500000_i64
    });

    // Parse CDC event
    let cdc_event =
        DebeziumEvent::from_json_value(&cdc_json).expect("Failed to parse update event");

    // Convert to workflow input
    let workflow_input = cdc_event.to_workflow_input();

    // Verify operation type
    assert_eq!(workflow_input["operation"], "update");

    // Verify changed fields were detected
    assert!(workflow_input["changed_fields"].is_array());
    let changed_fields = workflow_input["changed_fields"].as_array().unwrap();
    assert_eq!(changed_fields.len(), 2);

    // Verify the changed field names
    let changed_names: Vec<String> = changed_fields
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(changed_names.contains(&"email".to_string()));
    assert!(changed_names.contains(&"credit_limit".to_string()));

    // Verify before/after states are preserved
    assert!(workflow_input["before"].is_object());
    assert!(workflow_input["after"].is_object());
    assert_eq!(workflow_input["before"]["email"], "bob@example.com");
    assert_eq!(workflow_input["after"]["email"], "bob.smith@example.com");

    // Create workflow that routes based on update operations
    // Note: Condition::contains for arrays would require a custom evaluator
    // For this test, we use operation="update" as the routing condition
    let routes = vec![Route::new(
        "email_change_route",
        "Email Change Handler",
        Condition::equals("operation", "update"),
        vec![Action::Log {
            level: "info".to_string(),
            message: "Email address was updated".to_string(),
        }],
    )];

    let workflow = Workflow::new("account_update_workflow", "Account Update Handler", routes);

    // Route the update event
    let route_match = WorkflowRouter::select_route(&workflow, &workflow_input)
        .expect("Failed to route update event");

    // Verify the route matched based on changed fields
    assert!(route_match.is_some());
    let route_match = route_match.unwrap();
    assert_eq!(route_match.route.id, "email_change_route");

    println!("✅ CDC update event with changed fields processed successfully");
}

#[tokio::test]
async fn test_cdc_position_tracking() {
    // Create CDC event with position metadata
    let cdc_json = json!({
        "after": {
            "id": 789,
            "product_name": "Widget",
            "price": 29.99
        },
        "op": "c",
        "source": {
            "connector": "postgresql",
            "db": "inventory",
            "schema": "public",
            "table": "products",
            "txId": 999888,
            "lsn": 55566677,
            "ts_ms": 1698765600000_i64
        }
    });

    let cdc_event = DebeziumEvent::from_json_value(&cdc_json).expect("Failed to parse event");

    // Get CDC position for lineage tracking
    let position = cdc_event.get_cdc_position();

    // Verify position metadata
    assert_eq!(position.get("lsn"), Some(&"55566677".to_string()));
    assert_eq!(position.get("connector"), Some(&"postgresql".to_string()));
    assert_eq!(position.get("tx_id"), Some(&"999888".to_string()));
    assert_eq!(position.get("table"), Some(&"products".to_string()));
    assert_eq!(position.get("database"), Some(&"inventory".to_string()));

    // Get qualified table name
    let table_name = cdc_event.get_qualified_table_name();
    assert_eq!(table_name, "inventory.public.products");

    println!("✅ CDC position tracking verified");
}
