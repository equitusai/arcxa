//! Workflow Conditional Router
//!
//! A rule-based routing engine that directs data processing through
//! different pipelines based on configurable conditions.
//!
//! ## Features
//!
//! - **Dynamic Routing**: Route data based on content, metadata, or external signals
//! - **Complex Conditions**: Support for comparisons, logical operators, nested expressions
//! - **Action Orchestration**: Execute transformations, validations, notifications
//! - **Workflow Versioning**: Track changes with immutable version history
//! - **REST API**: Complete HTTP API for workflow management
//!
//! ## Architecture
//!
//! The workflow system is organized in layers:
//!
//! ```text
//! API Layer        - REST endpoints for workflow management
//!   ↓
//! Engine Layer     - Stateless routing and execution logic
//!   ↓
//! Domain Layer     - Core business types and validation
//!   ↓
//! Storage Layer    - Workflow persistence and CRUD operations
//! ```
//!
//! ## Example Usage
//!
//! ```ignore
//! use graphica_coordinator::workflows::{
//!     domain::{Workflow, Route, Condition, Action},
//!     engine::{WorkflowRouter, ActionExecutor, ExecutionContext},
//!     storage::WorkflowStore,
//! };
//! use serde_json::json;
//!
//! # async fn example() -> anyhow::Result<()> {
//! // Create a workflow
//! let routes = vec![
//!     Route::with_priority(
//!         "high_priority",
//!         "High Priority Route",
//!         Condition::equals("priority", "high"),
//!         vec![
//!             Action::SendToKafka {
//!                 topic: "high_priority_queue".to_string(),
//!                 partition_key: None,
//!             },
//!             Action::RecordLineage {
//!                 event_type: "routing".to_string(),
//!                 metadata: json!({"route": "high"}),
//!             },
//!         ],
//!         100,
//!     ),
//! ];
//!
//! let workflow = Workflow::new("wf_001", "Priority Router", routes);
//!
//! // Store the workflow
//! let store = WorkflowStore::new();
//! store.create(workflow.clone())?;
//!
//! // Execute the workflow
//! let input_data = json!({"priority": "high", "data": "test"});
//! let route_match = WorkflowRouter::select_route(&workflow, &input_data)?;
//!
//! if let Some(matched) = route_match {
//!     let mut data = input_data.clone();
//!     let context = ExecutionContext {
//!         workflow_id: workflow.id.clone(),
//!         route_id: matched.route.id.clone(),
//!         input_data: input_data.clone(),
//!     };
//!
//!     let results = ActionExecutor::execute_actions(
//!         &matched.route.actions,
//!         &mut data,
//!         &context,
//!     ).await?;
//!
//!     println!("Executed {} actions", results.len());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## API Endpoints
//!
//! The production REST workflow surface is mounted via
//! `crate::api::workflow::create_workflow_router()` inside the coordinator REST API.
//! That router owns the `/api/v1/workflows/*` contract, including validation,
//! schedule management, execution history, and progress monitoring.

pub mod api;
pub mod cancellation_manager;
pub mod cicd;
pub mod cli;
pub mod dataset_input;
pub mod db_extract_callback;
pub mod db_loader_callback;
pub mod declarative;
pub mod deployment;
pub mod domain;
pub mod engine;
pub mod git;
pub mod governance;
pub mod integration;
pub mod lineage;
pub mod ontology;
pub mod storage;
pub mod testing;
pub mod utils;

// Re-export commonly used types
pub use api::handlers::{ApiError, WorkflowApiState};
pub use cancellation_manager::CancellationManager;
pub use domain::{
    Action, ActionResult, ActionStatus, Condition, Route, Workflow, WorkflowId, WorkflowSummary,
};
pub use engine::{ActionExecutor, ConditionEvaluator, ExecutionContext, WorkflowRouter};
pub use storage::WorkflowStore;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_end_to_end_workflow() {
        // Create workflow store
        let store = WorkflowStore::new();

        // Create a workflow
        let routes = vec![
            Route::with_priority(
                "enterprise",
                "Enterprise Route",
                Condition::and(vec![
                    Condition::equals("customer_type", "enterprise"),
                    Condition::greater_than("annual_revenue", 1000000),
                ]),
                vec![
                    Action::SetField {
                        field: "tier".to_string(),
                        value: json!("platinum"),
                    },
                    Action::SendToKafka {
                        topic: "enterprise_customers".to_string(),
                        partition_key: Some("customer_id".to_string()),
                    },
                ],
                100,
            ),
            Route::new(
                "standard",
                "Standard Route",
                Condition::Always,
                vec![Action::SetField {
                    field: "tier".to_string(),
                    value: json!("standard"),
                }],
            ),
        ];

        let workflow = Workflow::new("wf_customer_routing", "Customer Routing", routes);
        store.create(workflow.clone()).unwrap();

        // Test enterprise customer
        let input = json!({
            "customer_type": "enterprise",
            "annual_revenue": 5000000,
            "name": "Acme Corp"
        });

        let route_match = WorkflowRouter::select_route(&workflow, &input).unwrap();
        assert!(route_match.is_some());
        assert_eq!(route_match.as_ref().unwrap().route.id, "enterprise");

        // Execute actions
        let mut data = input.clone();
        let context = ExecutionContext {
            workflow_id: workflow.id.clone(),
            route_id: route_match.as_ref().unwrap().route.id.clone(),
            input_data: input.clone(),
            rule_executor: None,
            transformer_registry: None,
            kafka_producer: None,
            http_client: None,
            lineage_generator: None,
            manual_mapping_store: None,
            execution_id: None,
            action_index: 0,
            metrics: None,
            approval_store: None,
            execution_store: None,
            column_lineage_store: None,
            tenant_id: "default".to_string(),
            timeout_config:
                graphica_core::orchestration::workflow::config::ExecutionTimeout::default(),
            workflow_start_time: Instant::now(),
            stage_start_time: Arc::new(RwLock::new(None)),
            db2_pool: None,
            postgres_pool: None,
            memory_monitor: None,
        };

        let results = ActionExecutor::execute_actions(
            &route_match.unwrap().route.actions,
            &mut data,
            &context,
        )
        .await
        .unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(data["tier"], "platinum");

        // Test standard customer
        let input = json!({
            "customer_type": "small_business",
            "annual_revenue": 50000,
            "name": "Small Co"
        });

        let route_match = WorkflowRouter::select_route(&workflow, &input).unwrap();
        assert!(route_match.is_some());
        assert_eq!(route_match.as_ref().unwrap().route.id, "standard");
    }

    #[test]
    fn test_workflow_validation() {
        // Valid workflow
        let routes = vec![Route::new(
            "test",
            "Test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
        )];

        let workflow = Workflow::new("wf_test", "Test", routes);
        assert!(workflow.validate().is_ok());

        // Invalid workflow (no routes)
        let workflow = Workflow::new("wf_invalid", "Invalid", vec![]);
        assert!(workflow.validate().is_err());
    }
}
