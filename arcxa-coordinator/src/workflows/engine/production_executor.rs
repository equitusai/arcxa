//! Production workflow executor using graphica-core's WorkflowEngine
//!
//! This module provides integration between the coordinator's workflow system
//! and the production-ready WorkflowEngine from graphica-core.

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use graphica_core::catalog::DataSourceCatalog;
use graphica_core::orchestration::ml::ModelInvoker;
use graphica_core::orchestration::rules::RuleExecutor;
use graphica_core::orchestration::workflow::{
    definition::{ConfidenceGateConfig, FallbackStrategy, HeuristicConfig, MLPredictionConfig},
    executor::{ExecutionContext, TransformerCallback, WorkflowExecutor, WorkflowResult},
    traits::{RuntimeHealth, WorkflowRuntime},
    LineageTracker, StepConfig, StepType, WorkflowDefinition, WorkflowStep,
};
use graphica_core::secrets::providers::SecretStoreRegistry;

use crate::governance::rdf_store::GraphicaRdfStore;
use crate::workflows::db_extract_callback::create_db_extract_callback;
use crate::workflows::engine::transformers::TransformerRegistry;

/// Production workflow executor
pub struct ProductionWorkflowExecutor {
    model_invoker: Arc<ModelInvoker>,
    rule_executor: Arc<RuleExecutor>,
    /// Optional transformer registry for ETL step execution
    transformer_registry: Option<Arc<TransformerRegistry>>,
    /// Optional lineage tracker for automatic immutable lineage tracking
    lineage_tracker: Option<Arc<dyn LineageTracker>>,
    /// Optional datasource catalog for DB loader step execution
    datasource_catalog: Option<Arc<dyn DataSourceCatalog>>,
    /// Optional RDF store for ontology-driven loading
    rdf_store: Option<Arc<GraphicaRdfStore>>,
    /// Optional secret store registry for resolving datasource credentials
    secret_store_registry: Option<Arc<SecretStoreRegistry>>,
}

impl ProductionWorkflowExecutor {
    /// Create new production executor with required dependencies
    pub fn new(model_invoker: Arc<ModelInvoker>, rule_executor: Arc<RuleExecutor>) -> Self {
        Self {
            model_invoker,
            rule_executor,
            transformer_registry: None,
            lineage_tracker: None,
            datasource_catalog: None,
            rdf_store: None,
            secret_store_registry: None,
        }
    }

    /// Set the transformer registry for ETL step execution
    ///
    /// This allows SemanticMapper steps to use the real OntologyMapperTransformer
    /// with column lineage support instead of the stub implementation.
    pub fn with_transformer_registry(mut self, registry: Arc<TransformerRegistry>) -> Self {
        self.transformer_registry = Some(registry);
        self
    }

    /// Set the lineage tracker for automatic immutable lineage tracking
    ///
    /// This enables automatic row-level lineage tracking for every workflow execution.
    /// Each transformation step will automatically record lineage events to the RDF store.
    pub fn with_lineage_tracker(mut self, tracker: Arc<dyn LineageTracker>) -> Self {
        self.lineage_tracker = Some(tracker);
        self
    }

    /// Set the datasource catalog for DB loader step execution
    ///
    /// This allows DbLoader steps to look up datasource configurations and load data
    /// to databases (DB2, PostgreSQL, Oracle, etc.) instead of using the stub implementation.
    pub fn with_datasource_catalog(mut self, catalog: Arc<dyn DataSourceCatalog>) -> Self {
        self.datasource_catalog = Some(catalog);
        self
    }

    /// Set the RDF store for ontology-driven loading
    ///
    /// This enables automatic entity-based loading where table schemas are generated
    /// from ontology definitions stored in the RDF store.
    pub fn with_rdf_store(mut self, rdf_store: Arc<GraphicaRdfStore>) -> Self {
        self.rdf_store = Some(rdf_store);
        self
    }

    /// Set secret store registry for resolving datasource credentials
    pub fn with_secret_store_registry(mut self, registry: Arc<SecretStoreRegistry>) -> Self {
        self.secret_store_registry = Some(registry);
        self
    }

    /// Create a transformer callback that wraps the TransformerRegistry
    ///
    /// Note: This uses a wrapper type to safely pass a raw pointer across async boundaries.
    /// This is safe because:
    /// 1. The callback is awaited immediately in execute_semantic_mapper
    /// 2. The data reference outlives the future
    /// 3. No other code accesses data during the await
    fn create_transformer_callback(registry: Arc<TransformerRegistry>) -> Arc<TransformerCallback> {
        Arc::new(Box::new(
            move |name: &str, config: &serde_json::Value, data: &mut serde_json::Value| {
                // Clone values for the async block
                let registry = registry.clone();
                let name = name.to_string();
                let config = config.clone();

                // Wrap the raw pointer in a Send-able wrapper
                // SAFETY: We ensure the pointer remains valid by awaiting immediately
                let data_ptr = SendPtr(data as *mut serde_json::Value);

                Box::pin(async move {
                    // Move the SendPtr into the async block to satisfy Send bounds
                    let ptr = data_ptr;

                    // SAFETY: This is safe because the caller (execute_semantic_mapper) awaits
                    // this future immediately and the data reference remains valid throughout
                    let data = unsafe { &mut *ptr.0 };

                    registry.execute(&name, &config, data, None).await
                })
            },
        ))
    }

    /// Execute a workflow definition with given input
    ///
    /// # Arguments
    /// * `definition` - Workflow definition to execute
    /// * `input` - Input data for the workflow
    /// * `resource_limits` - Optional resource limits for execution (Proposal 5 - Memory Management)
    pub async fn execute_workflow(
        &self,
        definition: WorkflowDefinition,
        input: JsonValue,
        resource_limits: Option<graphica_core::orchestration::workflow::ResourceLimits>,
    ) -> Result<WorkflowResult> {
        // Create executor for this workflow with automatic lineage tracking
        let mut executor = if let Some(tracker) = &self.lineage_tracker {
            WorkflowExecutor::with_lineage(
                definition,
                self.model_invoker.clone(),
                self.rule_executor.clone(),
                tracker.clone(),
            )
            .context("Failed to create workflow executor with lineage")?
        } else {
            WorkflowExecutor::new(
                definition,
                self.model_invoker.clone(),
                self.rule_executor.clone(),
            )
            .context("Failed to create workflow executor")?
        };

        // Wire transformer callback if registry is available
        if let Some(registry) = &self.transformer_registry {
            let callback = Self::create_transformer_callback(registry.clone());
            executor = executor.with_transformer_callback(callback);
        }

        // Wire DB loader callback if datasource catalog is available
        if let Some(catalog) = &self.datasource_catalog {
            let db_loader_callback =
                crate::workflows::db_loader_callback::create_db_loader_callback(
                    catalog.clone(),
                    self.rdf_store.clone(),
                    self.secret_store_registry.clone(),
                );
            executor = executor.with_db_loader_callback(db_loader_callback);

            let db_extract_callback = create_db_extract_callback(catalog.clone());
            executor = executor.with_db_extract_callback(db_extract_callback);
        }

        // Create execution context with optional resource limits
        let mut context = ExecutionContext::new(input);
        if let Some(limits) = resource_limits {
            context = context.with_resource_limits(limits);
        }

        // Execute workflow
        executor
            .execute(context)
            .await
            .context("Workflow execution failed")
    }

    /// Check if a workflow can be executed by the production executor
    /// Returns true if the workflow contains ML or rule execution steps
    pub fn can_execute(definition: &WorkflowDefinition) -> bool {
        definition.steps.iter().any(|step| {
            matches!(
                step.step_type,
                StepType::MlPrediction | StepType::HeuristicRule | StepType::WasmRule
            )
        })
    }
}

/// Wrapper to make a raw pointer Send
///
/// SAFETY: This is only safe when:
/// 1. The pointee outlives the future
/// 2. The future is awaited immediately (no concurrent access)
/// 3. Only one reference to the data exists at a time
struct SendPtr(*mut serde_json::Value);

// SAFETY: We guarantee single-threaded access and immediate await
unsafe impl Send for SendPtr {}

/// Implement WorkflowRuntime trait for trait-based execution
///
/// This implementation delegates to the core's WorkflowExecutor for now.
/// In the future, this will be replaced with a production streaming executor
/// using Timely/Differential dataflow.
#[async_trait]
impl WorkflowRuntime for ProductionWorkflowExecutor {
    async fn execute_workflow(
        &self,
        definition: WorkflowDefinition,
        input: ExecutionContext,
    ) -> Result<WorkflowResult> {
        // Create executor for this workflow with automatic lineage tracking
        let mut executor = if let Some(tracker) = &self.lineage_tracker {
            WorkflowExecutor::with_lineage(
                definition,
                self.model_invoker.clone(),
                self.rule_executor.clone(),
                tracker.clone(),
            )
            .context("Failed to create workflow executor with lineage")?
        } else {
            WorkflowExecutor::new(
                definition,
                self.model_invoker.clone(),
                self.rule_executor.clone(),
            )
            .context("Failed to create workflow executor")?
        };

        // Wire transformer callback if registry is available
        if let Some(registry) = &self.transformer_registry {
            let callback = Self::create_transformer_callback(registry.clone());
            executor = executor.with_transformer_callback(callback);
        }

        // Wire DB loader callback if datasource catalog is available
        if let Some(catalog) = &self.datasource_catalog {
            let db_loader_callback =
                crate::workflows::db_loader_callback::create_db_loader_callback(
                    catalog.clone(),
                    self.rdf_store.clone(),
                    self.secret_store_registry.clone(),
                );
            executor = executor.with_db_loader_callback(db_loader_callback);

            let db_extract_callback = create_db_extract_callback(catalog.clone());
            executor = executor.with_db_extract_callback(db_extract_callback);
        }

        // Execute workflow using the core executor
        executor
            .execute(input)
            .await
            .context("Workflow execution failed")
    }

    fn runtime_name(&self) -> &str {
        "ProductionWorkflowExecutor"
    }

    fn health_check(&self) -> Result<RuntimeHealth> {
        // Basic health check - verify dependencies are available
        // TODO: Add more sophisticated checks (model service, rule engine, etc.)
        Ok(RuntimeHealth::Healthy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use graphica_core::orchestration::ml::{CacheConfig, ModelCache, ModelRegistry};

    fn create_test_executor() -> ProductionWorkflowExecutor {
        let registry = Arc::new(ModelRegistry::new());
        let cache = Arc::new(ModelCache::new(CacheConfig::default()));
        let model_invoker = Arc::new(ModelInvoker::new(registry, cache).unwrap());
        let rule_executor = Arc::new(RuleExecutor::new());

        ProductionWorkflowExecutor::new(model_invoker, rule_executor)
    }

    #[test]
    fn test_can_execute_ml_workflow() {
        let definition = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "predict".to_string(),
                step_type: StepType::MlPrediction,
                config: StepConfig::MLPrediction(MLPredictionConfig {
                    model_id: "test_model".to_string(),
                    model_version: "1.0.0".to_string(),
                    features: vec!["feature1".to_string()],
                    feature_mappings: vec![],
                    predictions: vec![
                        graphica_core::orchestration::workflow::definition::PredictionSpec {
                            attribute_name: "test_prediction".to_string(),
                            mock_value: "test_value".to_string(),
                            mock_confidence: 0.9,
                        },
                    ],
                    confidence_threshold: None,
                    timeout_ms: 5000,
                    cache_ttl_secs: None,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::ManualReview,
        };

        assert!(ProductionWorkflowExecutor::can_execute(&definition));
    }

    #[test]
    fn test_can_execute_rule_workflow() {
        let definition = WorkflowDefinition {
            steps: vec![WorkflowStep {
                id: "rule1".to_string(),
                step_type: StepType::HeuristicRule,
                config: StepConfig::Heuristic(HeuristicConfig {
                    rule_id: "test_rule".to_string(),
                    min_confidence: 0.7,
                }),
                depends_on: vec![],
            }],
            fusion_threshold: 0.8,
            fallback: FallbackStrategy::RejectFusion,
        };

        assert!(ProductionWorkflowExecutor::can_execute(&definition));
    }

    #[test]
    fn test_cannot_execute_simple_workflow() {
        let definition = WorkflowDefinition {
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
        };

        // ConfidenceGate-only workflows don't need production executor
        assert!(!ProductionWorkflowExecutor::can_execute(&definition));
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let _executor = create_test_executor();
        // Just verify it can be created
    }

    #[tokio::test]
    async fn test_workflow_runtime_trait() {
        use graphica_core::orchestration::workflow::executor::FinalDecision;
        use graphica_core::orchestration::workflow::traits::WorkflowRuntime;

        let executor = create_test_executor();

        // Test runtime_name
        assert_eq!(executor.runtime_name(), "ProductionWorkflowExecutor");

        // Test health_check
        let health = executor.health_check().unwrap();
        assert_eq!(health, RuntimeHealth::Healthy);

        // Test execute_workflow via trait
        let definition = WorkflowDefinition {
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
        };

        let context = ExecutionContext::new(serde_json::json!({"confidence": 0.7}));

        // Call trait method explicitly using WorkflowRuntime::execute_workflow
        let result = WorkflowRuntime::execute_workflow(&executor, definition, context)
            .await
            .unwrap();

        // Should succeed with confidence above threshold
        assert!(result.success);
        // With just a ConfidenceGate and no fusion steps, workflow uses fallback strategy
        assert_eq!(result.final_decision, FinalDecision::ManualReview);
    }
}
