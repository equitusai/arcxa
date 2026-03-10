//! Workflow API support
//!
//! Shared handlers, DTOs, and state adapters for workflow management and execution.

use std::sync::Arc;

use handlers::WorkflowApiState;

pub mod approval_handlers;
pub mod batch_handlers;
pub mod dto;
pub mod handlers;
// TODO: Integrate RDF endpoints after completing Phase 3 foundation
// pub mod rdf_endpoints;
pub mod sse;
pub mod stream_handlers;

/// Implement FromRef to allow extracting WorkflowApiState from ApiState
/// This enables handlers to use `State<WorkflowApiState>` while the router uses `ApiState`
impl axum::extract::FromRef<Arc<crate::api::ApiState>> for WorkflowApiState {
    fn from_ref(state: &Arc<crate::api::ApiState>) -> Self {
        WorkflowApiState {
            store: state
                .workflow_store
                .clone()
                .expect("workflow_store not initialized"),
            execution_store: state
                .execution_store
                .clone()
                .expect("execution_store not initialized"),
            schedule_store: state
                .schedule_store
                .clone()
                .expect("schedule_store not initialized"),
            model_invoker: state.model_registry.as_ref().and_then(|registry| {
                state.model_cache.as_ref().map(|cache| {
                    // Get core registry from persistent wrapper
                    let core_registry = registry.as_core_registry();
                    Arc::new(
                        graphica_core::orchestration::ml::ModelInvoker::new(
                            core_registry,
                            cache.clone(),
                        )
                        .expect("Failed to create ModelInvoker"),
                    )
                })
            }),
            rule_executor: state.rule_executor.clone(),
            transformer_registry: state.transformer_registry.clone(),
            kafka_producer: state.kafka_producer.clone(),
            http_client: state.http_client.clone(),
            lineage_generator: state.lineage_generator.clone(),
            manual_mapping_store: None,
            metrics: state.metrics.clone(),
            policy_checker: state.policy_checker.clone(),
            execution_sync: state.execution_sync.clone(),
            column_lineage_store: state.column_lineage_store.clone(),
        }
    }
}

/// Implement FromRef to allow extracting WorkflowApiState from Arc<WorkflowApiState>
/// This is needed for routers that use Arc<WorkflowApiState> directly as their state
impl axum::extract::FromRef<Arc<WorkflowApiState>> for WorkflowApiState {
    fn from_ref(state: &Arc<WorkflowApiState>) -> Self {
        // Clone the Arc'd fields (cheap operation)
        WorkflowApiState {
            store: state.store.clone(),
            execution_store: state.execution_store.clone(),
            schedule_store: state.schedule_store.clone(),
            model_invoker: state.model_invoker.clone(),
            rule_executor: state.rule_executor.clone(),
            transformer_registry: state.transformer_registry.clone(),
            kafka_producer: state.kafka_producer.clone(),
            http_client: state.http_client.clone(),
            lineage_generator: state.lineage_generator.clone(),
            manual_mapping_store: None,
            metrics: state.metrics.clone(),
            policy_checker: state.policy_checker.clone(),
            execution_sync: state.execution_sync.clone(),
            column_lineage_store: state.column_lineage_store.clone(),
        }
    }
}
