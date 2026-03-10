//! Integration tests for WorkflowInput system
//!
//! Tests the complete workflow input adapter flow including:
//! - JsonInputAdapter (backward compatibility)
//! - SparqlInputAdapter with mock QueryExecutor
//! - EntityFilterAdapter with query generation
//! - Batched execution with multiple contexts
//! - execute_workflow_with_input method

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
        input::{
            EntityFilterAdapter, InputAdapter, JsonInputAdapter, QueryExecutor, SparqlInputAdapter,
            WorkflowInput,
        },
    },
};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// Mock QueryExecutor for testing SPARQL execution
struct MockQueryExecutor {
    /// Canned responses for queries
    responses: HashMap<String, Vec<JsonValue>>,
}

impl MockQueryExecutor {
    fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }

    fn add_response(&mut self, query_pattern: &str, response: Vec<JsonValue>) {
        self.responses.insert(query_pattern.to_string(), response);
    }

    /// Create a mock with default customer data
    fn with_customer_data() -> Self {
        let mut mock = Self::new();

        // Mock response for customer query
        let customers = vec![
            serde_json::json!({"customer": "http://example.com/customer/1", "name": "Alice"}),
            serde_json::json!({"customer": "http://example.com/customer/2", "name": "Bob"}),
            serde_json::json!({"customer": "http://example.com/customer/3", "name": "Charlie"}),
        ];

        mock.add_response("SELECT", customers);
        mock
    }
}

#[async_trait::async_trait]
impl QueryExecutor for MockQueryExecutor {
    async fn execute_query(&self, query: &str, _graph: Option<&str>) -> Result<Vec<JsonValue>> {
        // Find matching response by query pattern
        for (pattern, response) in &self.responses {
            if query.contains(pattern) {
                return Ok(response.clone());
            }
        }

        // No match - return empty
        Ok(vec![])
    }
}

/// Helper to create a test ModelInvoker
fn create_test_model_invoker() -> Result<Arc<ModelInvoker>> {
    let registry = Arc::new(ModelRegistry::new());
    let cache_config = CacheConfig {
        max_size: 10,
        default_ttl: std::time::Duration::from_secs(60),
        model_ttls: HashMap::new(),
    };
    let cache = Arc::new(ModelCache::new(cache_config));
    let invoker = ModelInvoker::new(registry, cache)?;
    Ok(Arc::new(invoker))
}

/// Helper to create a test RuleExecutor
fn create_test_rule_executor() -> Arc<RuleExecutor> {
    Arc::new(RuleExecutor::new())
}

/// Helper to create a simple test workflow
fn create_test_workflow() -> WorkflowDefinition {
    WorkflowDefinition {
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
    }
}

#[tokio::test]
async fn test_json_input_adapter() -> Result<()> {
    // Create adapter
    let adapter = JsonInputAdapter;

    // Test JSON input
    let input = WorkflowInput::Json {
        data: serde_json::json!({
            "customer_id": "cust_123",
            "name": "Test Customer",
            "confidence": 0.95
        }),
    };

    // Prepare context
    let contexts = adapter.prepare_context(&input).await?;

    // Verify single context created
    assert_eq!(contexts.len(), 1, "JSON input should create single context");

    let context = &contexts[0];
    assert_eq!(
        context.input_data["customer_id"], "cust_123",
        "Input data should be preserved"
    );
    assert_eq!(
        context.input_data["confidence"], 0.95,
        "Confidence should be preserved"
    );

    Ok(())
}

#[tokio::test]
async fn test_sparql_input_adapter_single_batch() -> Result<()> {
    // Create mock query executor
    let mock_executor = MockQueryExecutor::with_customer_data();
    let query_executor = Arc::new(mock_executor);

    // Create adapter
    let adapter = SparqlInputAdapter::new(query_executor);

    // Test SPARQL input with small result set (single batch)
    let input = WorkflowInput::SparqlQuery {
        query: "SELECT ?customer ?name WHERE { ?customer a gph:Customer ; gph:name ?name }"
            .to_string(),
        graph: Some("http://graphica.io/latest".to_string()),
        batch_size: Some(100), // Larger than result set
        limit: None,
    };

    // Prepare contexts
    let contexts = adapter.prepare_context(&input).await?;

    // Verify single batch (3 results < 100 batch size)
    assert_eq!(
        contexts.len(),
        1,
        "Should create single batch for 3 results"
    );

    let context = &contexts[0];
    assert!(
        context.input_data.is_array(),
        "Input should be array of results"
    );

    let results = context.input_data.as_array().unwrap();
    assert_eq!(results.len(), 3, "Should have 3 customer results");
    assert_eq!(results[0]["name"], "Alice");
    assert_eq!(results[1]["name"], "Bob");
    assert_eq!(results[2]["name"], "Charlie");

    Ok(())
}

#[tokio::test]
async fn test_sparql_input_adapter_multiple_batches() -> Result<()> {
    // Create mock with 250 results
    let mut mock_executor = MockQueryExecutor::new();
    let large_result_set: Vec<JsonValue> = (0..250)
        .map(|i| {
            serde_json::json!({
                "customer": format!("http://example.com/customer/{}", i),
                "name": format!("Customer {}", i)
            })
        })
        .collect();

    mock_executor.add_response("SELECT", large_result_set);
    let query_executor = Arc::new(mock_executor);

    // Create adapter
    let adapter = SparqlInputAdapter::new(query_executor);

    // Test with batch size of 100
    let input = WorkflowInput::SparqlQuery {
        query: "SELECT ?customer WHERE { ?customer a gph:Customer }".to_string(),
        graph: None,
        batch_size: Some(100),
        limit: None,
    };

    // Prepare contexts
    let contexts = adapter.prepare_context(&input).await?;

    // Verify 3 batches (250 results / 100 batch size = 3 batches: 100 + 100 + 50)
    assert_eq!(contexts.len(), 3, "Should create 3 batches for 250 results");

    // Check first batch
    let batch1 = contexts[0].input_data.as_array().unwrap();
    assert_eq!(batch1.len(), 100, "First batch should have 100 items");

    // Check second batch
    let batch2 = contexts[1].input_data.as_array().unwrap();
    assert_eq!(batch2.len(), 100, "Second batch should have 100 items");

    // Check third batch (remainder)
    let batch3 = contexts[2].input_data.as_array().unwrap();
    assert_eq!(batch3.len(), 50, "Third batch should have 50 items");

    Ok(())
}

#[tokio::test]
async fn test_sparql_input_adapter_with_limit() -> Result<()> {
    // Create mock with 1000 results
    let mut mock_executor = MockQueryExecutor::new();
    let large_result_set: Vec<JsonValue> =
        (0..1000).map(|i| serde_json::json!({"id": i})).collect();

    mock_executor.add_response("SELECT", large_result_set);
    let query_executor = Arc::new(mock_executor);

    // Create adapter
    let adapter = SparqlInputAdapter::new(query_executor);

    // Test with limit
    let input = WorkflowInput::SparqlQuery {
        query: "SELECT ?entity WHERE { ?entity a gph:Entity }".to_string(),
        graph: None,
        batch_size: Some(100),
        limit: Some(250), // Limit to 250 results
    };

    // Prepare contexts
    let contexts = adapter.prepare_context(&input).await?;

    // Verify limit applied (250 results / 100 batch size = 3 batches)
    assert_eq!(
        contexts.len(),
        3,
        "Should create 3 batches for 250 limited results"
    );

    // Verify total item count
    let total_items: usize = contexts
        .iter()
        .map(|ctx| ctx.input_data.as_array().unwrap().len())
        .sum();
    assert_eq!(total_items, 250, "Should have exactly 250 items total");

    Ok(())
}

#[tokio::test]
async fn test_entity_filter_adapter() -> Result<()> {
    // Create mock query executor
    let mut mock_executor = MockQueryExecutor::new();
    let customers = vec![
        serde_json::json!({"entity": "http://example.com/customer/1"}),
        serde_json::json!({"entity": "http://example.com/customer/2"}),
    ];
    mock_executor.add_response("gph:Customer", customers);
    let query_executor = Arc::new(mock_executor);

    // Create adapter
    let adapter = EntityFilterAdapter::new(query_executor);

    // Test entity filter
    let input = WorkflowInput::EntityFilter {
        entity_type: "gph:Customer".to_string(),
        graph: Some("http://graphica.io/latest".to_string()),
        created_after: Some("2025-10-01T00:00:00Z".to_string()),
        updated_after: None,
        limit: Some(1000),
        batch_size: Some(100),
    };

    // Prepare contexts
    let contexts = adapter.prepare_context(&input).await?;

    // Verify contexts created
    assert_eq!(
        contexts.len(),
        1,
        "Should create single batch for 2 results"
    );

    let context = &contexts[0];
    let results = context.input_data.as_array().unwrap();
    assert_eq!(results.len(), 2, "Should have 2 customer entities");

    Ok(())
}

#[tokio::test]
async fn test_execute_workflow_with_json_input() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register workflow
    let workflow_id = "test_json_input_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "JSON Input Test".to_string(),
            create_test_workflow(),
            None,
            vec![],
        )
        .await?;

    // Create JSON input
    let input = WorkflowInput::Json {
        data: serde_json::json!({
            "customer_id": "cust_123",
            "confidence": 0.95
        }),
    };

    // Create adapter
    let adapter: Arc<dyn InputAdapter> = Arc::new(JsonInputAdapter);

    // Execute workflow
    let context = HashMap::new();
    let results = engine
        .execute_workflow_with_input(workflow_id, input, adapter, &context)
        .await?;

    // Verify single result
    assert_eq!(results.len(), 1, "Should have single result for JSON input");
    assert!(results[0].success, "Execution should succeed");
    assert!(
        !results[0].execution_id.is_empty(),
        "Should have execution ID"
    );

    Ok(())
}

#[tokio::test]
async fn test_execute_workflow_with_sparql_input_batched() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register workflow
    let workflow_id = "test_sparql_input_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "SPARQL Input Test".to_string(),
            create_test_workflow(),
            None,
            vec![],
        )
        .await?;

    // Create mock with 150 results (will create 2 batches with batch_size=100)
    let mut mock_executor = MockQueryExecutor::new();
    let result_set: Vec<JsonValue> = (0..150)
        .map(|i| {
            serde_json::json!({
                "customer": format!("http://example.com/customer/{}", i),
                "confidence": 0.9
            })
        })
        .collect();
    mock_executor.add_response("SELECT", result_set);
    let query_executor = Arc::new(mock_executor);

    // Create SPARQL input
    let input = WorkflowInput::SparqlQuery {
        query: "SELECT ?customer WHERE { ?customer a gph:Customer }".to_string(),
        graph: None,
        batch_size: Some(100),
        limit: None,
    };

    // Create adapter
    let adapter: Arc<dyn InputAdapter> = Arc::new(SparqlInputAdapter::new(query_executor));

    // Execute workflow
    let context = HashMap::new();
    let results = engine
        .execute_workflow_with_input(workflow_id, input, adapter, &context)
        .await?;

    // Verify batched results (150 results / 100 batch size = 2 batches)
    assert_eq!(results.len(), 2, "Should have 2 batched results");

    // Verify executions happened (may or may not succeed depending on step type compatibility)
    assert!(
        !results[0].execution_id.is_empty(),
        "First batch should have execution ID"
    );
    assert!(
        !results[1].execution_id.is_empty(),
        "Second batch should have execution ID"
    );

    // Verify unique execution IDs
    assert_ne!(
        results[0].execution_id, results[1].execution_id,
        "Each batch should have unique execution ID"
    );

    // Verify batched execution completed
    assert_eq!(
        results[0].step_results.len(),
        1,
        "First batch should have executed one step"
    );
    assert_eq!(
        results[1].step_results.len(),
        1,
        "Second batch should have executed one step"
    );

    Ok(())
}

#[tokio::test]
async fn test_execute_workflow_with_entity_filter_input() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register workflow
    let workflow_id = "test_entity_filter_workflow";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Entity Filter Test".to_string(),
            create_test_workflow(),
            None,
            vec![],
        )
        .await?;

    // Create mock
    let mock_executor = MockQueryExecutor::with_customer_data();
    let query_executor = Arc::new(mock_executor);

    // Create entity filter input
    let input = WorkflowInput::EntityFilter {
        entity_type: "gph:Customer".to_string(),
        graph: None,
        created_after: Some("2025-10-01T00:00:00Z".to_string()),
        updated_after: None,
        limit: None,
        batch_size: Some(100),
    };

    // Create adapter
    let adapter: Arc<dyn InputAdapter> = Arc::new(EntityFilterAdapter::new(query_executor));

    // Execute workflow
    let context = HashMap::new();
    let results = engine
        .execute_workflow_with_input(workflow_id, input, adapter, &context)
        .await?;

    // Verify single result (3 customers < 100 batch size)
    assert_eq!(results.len(), 1, "Should have single batch result");

    // Verify execution happened
    assert!(
        !results[0].execution_id.is_empty(),
        "Should have execution ID"
    );
    assert_eq!(
        results[0].step_results.len(),
        1,
        "Should have executed one step"
    );

    Ok(())
}

#[tokio::test]
async fn test_workflow_input_validation() -> Result<()> {
    // Test empty SPARQL query
    let invalid_sparql = WorkflowInput::SparqlQuery {
        query: "".to_string(),
        graph: None,
        batch_size: None,
        limit: None,
    };
    assert!(
        invalid_sparql.validate().is_err(),
        "Empty query should fail validation"
    );

    // Test non-SELECT query
    let invalid_sparql = WorkflowInput::SparqlQuery {
        query: "INSERT { ?s ?p ?o }".to_string(),
        graph: None,
        batch_size: None,
        limit: None,
    };
    assert!(
        invalid_sparql.validate().is_err(),
        "Non-SELECT query should fail validation"
    );

    // Test invalid batch size
    let invalid_sparql = WorkflowInput::SparqlQuery {
        query: "SELECT * WHERE { ?s ?p ?o }".to_string(),
        graph: None,
        batch_size: Some(0), // Invalid
        limit: None,
    };
    assert!(
        invalid_sparql.validate().is_err(),
        "Zero batch size should fail validation"
    );

    // Test empty entity type
    let invalid_filter = WorkflowInput::EntityFilter {
        entity_type: "".to_string(),
        graph: None,
        created_after: None,
        updated_after: None,
        limit: None,
        batch_size: None,
    };
    assert!(
        invalid_filter.validate().is_err(),
        "Empty entity type should fail validation"
    );

    // Test valid inputs
    let valid_sparql = WorkflowInput::SparqlQuery {
        query: "SELECT ?s WHERE { ?s ?p ?o }".to_string(),
        graph: None,
        batch_size: Some(100),
        limit: Some(1000),
    };
    assert!(
        valid_sparql.validate().is_ok(),
        "Valid SPARQL should pass validation"
    );

    let valid_filter = WorkflowInput::EntityFilter {
        entity_type: "gph:Customer".to_string(),
        graph: Some("http://graphica.io/latest".to_string()),
        created_after: Some("2025-10-01T00:00:00Z".to_string()),
        updated_after: None,
        limit: Some(5000),
        batch_size: Some(100),
    };
    assert!(
        valid_filter.validate().is_ok(),
        "Valid entity filter should pass validation"
    );

    let valid_json = WorkflowInput::Json {
        data: serde_json::json!({"test": "data"}),
    };
    assert!(
        valid_json.validate().is_ok(),
        "Valid JSON should pass validation"
    );

    Ok(())
}

#[tokio::test]
async fn test_workflow_input_execution_mode_detection() {
    // Test batch mode
    let sparql_batched = WorkflowInput::SparqlQuery {
        query: "SELECT ?s WHERE { ?s ?p ?o }".to_string(),
        graph: None,
        batch_size: Some(100),
        limit: None,
    };
    assert_eq!(
        format!("{:?}", sparql_batched.execution_mode()),
        "Batch",
        "SPARQL with batch_size should be Batch mode"
    );

    // Test single mode
    let json_input = WorkflowInput::Json {
        data: serde_json::json!({"test": "data"}),
    };
    assert_eq!(
        format!("{:?}", json_input.execution_mode()),
        "Single",
        "JSON input should be Single mode"
    );
}

#[tokio::test]
async fn test_context_metadata_passed_through() -> Result<()> {
    // Create engine
    let model_invoker = create_test_model_invoker()?;
    let rule_executor = create_test_rule_executor();
    let engine = WorkflowEngine::new_with_execution(model_invoker, rule_executor);

    // Register workflow
    let workflow_id = "test_context_metadata";
    engine
        .register_workflow(
            workflow_id.to_string(),
            "Context Metadata Test".to_string(),
            create_test_workflow(),
            None,
            vec![],
        )
        .await?;

    // Create input
    let input = WorkflowInput::Json {
        data: serde_json::json!({"confidence": 0.9}),
    };

    let adapter: Arc<dyn InputAdapter> = Arc::new(JsonInputAdapter);

    // Execute with context metadata
    let mut context = HashMap::new();
    context.insert("request_id".to_string(), "req_test_123".to_string());
    context.insert("initiator".to_string(), "integration_test".to_string());

    let results = engine
        .execute_workflow_with_input(workflow_id, input, adapter, &context)
        .await?;

    // Verify execution succeeded with context
    assert_eq!(results.len(), 1, "Should have one result");
    assert!(results[0].success, "Execution should succeed");

    Ok(())
}

#[tokio::test]
async fn test_data_source_query_validation() -> Result<()> {
    // Valid DataSourceQuery
    let valid = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: Some(1000),
        limit: Some(10000),
        timeout_secs: Some(60),
    };
    assert!(
        valid.validate().is_ok(),
        "Valid DataSourceQuery should pass"
    );

    // Empty source_id
    let invalid_source = WorkflowInput::DataSourceQuery {
        source_id: "".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: None,
        limit: None,
        timeout_secs: None,
    };
    assert!(
        invalid_source.validate().is_err(),
        "Empty source_id should fail"
    );

    // Empty query
    let invalid_query = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "".to_string(),
        parameters: None,
        batch_size: None,
        limit: None,
        timeout_secs: None,
    };
    assert!(invalid_query.validate().is_err(), "Empty query should fail");

    // Invalid batch size (0)
    let invalid_batch = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: Some(0),
        limit: None,
        timeout_secs: None,
    };
    assert!(
        invalid_batch.validate().is_err(),
        "Zero batch size should fail"
    );

    // Invalid batch size (too large)
    let invalid_batch_large = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: Some(20000),
        limit: None,
        timeout_secs: None,
    };
    assert!(
        invalid_batch_large.validate().is_err(),
        "Batch size > 10000 should fail"
    );

    // Invalid timeout (0)
    let invalid_timeout = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: None,
        limit: None,
        timeout_secs: Some(0),
    };
    assert!(
        invalid_timeout.validate().is_err(),
        "Zero timeout should fail"
    );

    // Invalid timeout (too large)
    let invalid_timeout_large = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: None,
        limit: None,
        timeout_secs: Some(1000),
    };
    assert!(
        invalid_timeout_large.validate().is_err(),
        "Timeout > 600 should fail"
    );

    Ok(())
}

#[tokio::test]
async fn test_data_source_query_execution_mode() {
    // With batch_size -> Batch mode
    let batched = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: Some(1000),
        limit: None,
        timeout_secs: None,
    };
    assert_eq!(
        format!("{:?}", batched.execution_mode()),
        "Batch",
        "DataSourceQuery with batch_size should be Batch mode"
    );

    // Without batch_size -> Single mode
    let single = WorkflowInput::DataSourceQuery {
        source_id: "urn:graphica:datasource:postgres_prod".to_string(),
        query: "SELECT * FROM customers".to_string(),
        parameters: None,
        batch_size: None,
        limit: None,
        timeout_secs: None,
    };
    assert_eq!(
        format!("{:?}", single.execution_mode()),
        "Single",
        "DataSourceQuery without batch_size should be Single mode"
    );
}
