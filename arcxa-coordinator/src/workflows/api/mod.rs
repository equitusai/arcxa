//! Workflow API support
//!
//! Shared handlers, DTOs, and state adapters for workflow management and execution.

use std::sync::Arc;

use handlers::WorkflowApiState;
use stream_handlers::StreamingApiState;

pub mod approval_handlers;
pub mod batch_handlers;
pub mod dto;
pub mod handlers;
// TODO: Integrate RDF endpoints after completing Phase 3 foundation
// pub mod rdf_endpoints;
pub mod sse;
pub mod stream_handlers;

pub fn build_stream_executor(
    workflow_store: Arc<crate::workflows::storage::WorkflowStore>,
    execution_store: Arc<crate::workflows::storage::ExecutionStore>,
    rule_executor: Option<Arc<graphica_core::orchestration::rules::RuleExecutor>>,
    transformer_registry: Option<Arc<crate::workflows::engine::transformers::TransformerRegistry>>,
    lineage_generator: Option<Arc<crate::workflows::lineage::WorkflowLineageGenerator>>,
    manual_mapping_store: Option<Arc<crate::mapping::manual::ManualMappingStore>>,
    column_lineage_store: Option<
        Arc<dyn graphica_core::core::lineage::column_level::ColumnLineageSink>,
    >,
    workflow_metrics: Option<Arc<crate::observability::metrics::WorkflowMetrics>>,
) -> Arc<crate::workflows::engine::StreamExecutor> {
    let mut stream_executor = crate::workflows::engine::StreamExecutor::new(
        workflow_store.clone(),
        execution_store.clone(),
    );

    if let Some(rule_executor) = rule_executor {
        stream_executor = crate::workflows::engine::StreamExecutor::with_rule_executor(
            workflow_store.clone(),
            execution_store.clone(),
            rule_executor,
        );
    }

    if let Some(transformer_registry) = transformer_registry {
        stream_executor = stream_executor.with_transformer_registry(transformer_registry);
    }

    if let Some(lineage_generator) = lineage_generator {
        stream_executor = stream_executor.with_lineage_generator(lineage_generator);
    }

    if let Some(manual_mapping_store) = manual_mapping_store {
        stream_executor = stream_executor.with_manual_mapping_store(manual_mapping_store);
    }

    if let Some(column_lineage_store) = column_lineage_store {
        stream_executor = stream_executor.with_column_lineage_store(column_lineage_store);
    }

    if let Some(workflow_metrics) = workflow_metrics {
        stream_executor = stream_executor.with_workflow_metrics(workflow_metrics);
    }

    Arc::new(stream_executor)
}

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

/// Allow extracting streaming workflow state from the shared API state.
impl axum::extract::FromRef<Arc<crate::api::ApiState>> for StreamingApiState {
    fn from_ref(state: &Arc<crate::api::ApiState>) -> Self {
        let workflow_store = state
            .workflow_store
            .clone()
            .expect("workflow_store not initialized");
        let execution_store = state
            .execution_store
            .clone()
            .expect("execution_store not initialized");

        StreamingApiState::from_shared(
            workflow_store,
            execution_store,
            state.stream_executor.clone().unwrap_or_else(|| {
                build_stream_executor(
                    state
                        .workflow_store
                        .clone()
                        .expect("workflow_store not initialized"),
                    state
                        .execution_store
                        .clone()
                        .expect("execution_store not initialized"),
                    state.rule_executor.clone(),
                    state.transformer_registry.clone(),
                    state.lineage_generator.clone(),
                    state.manual_mapping_store.clone(),
                    state.column_lineage_store.clone(),
                    state.metrics.clone(),
                )
            }),
            state.metrics.clone(),
        )
    }
}
