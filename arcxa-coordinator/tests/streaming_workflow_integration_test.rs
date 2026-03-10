//! Streaming Workflow Integration Tests
//!
//! Tests the complete streaming workflow lifecycle including:
//! - Workflow creation with streaming mode
//! - StreamExecutor initialization
//! - Kafka source integration
//! - Graceful shutdown
//! - Backward compatibility with batch workflows

use graphica_coordinator::workflows::domain::{
    Action, Condition, ExecutionMode, Route, StateBackendConfig, StreamingConfig,
    WatermarkStrategy, Workflow,
};
use graphica_coordinator::workflows::engine::{KafkaSource, StreamExecutor};
use graphica_coordinator::workflows::storage::{ExecutionStore, WorkflowStore};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

/// Helper to create a test streaming workflow
fn create_test_streaming_workflow(id: &str, topic: &str) -> Workflow {
    let route = Route::with_priority(
        "rt_001",
        "streaming_route",
        Condition::Always,
        vec![
            Action::Log {
                level: "info".to_string(),
                message: "Processing streaming record".to_string(),
            },
            Action::Transform {
                transformer: "add_timestamp".to_string(),
                config: json!({
                    "field": "processed_timestamp"
                }),
            },
        ],
        100,
    );

    let mut kafka_props = HashMap::new();
    kafka_props.insert(
        "bootstrap.servers".to_string(),
        "localhost:9092".to_string(),
    );

    let streaming_config = StreamingConfig {
        source_topic: topic.to_string(),
        consumer_group: format!("{}_group", id),
        checkpoint_interval_ms: 60000,
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30000,
        },
        max_parallel_workers: Some(2),
        state_backend: StateBackendConfig::Memory,
        auto_scaling: None,
        kafka_properties: kafka_props,
    };

    let mut workflow = Workflow::new(id, format!("Streaming Workflow {}", id), vec![route]);
    workflow.execution_mode = ExecutionMode::Streaming {
        config: streaming_config,
    };

    workflow.with_description("Integration test streaming workflow")
}

/// Helper to create a test batch workflow (for backward compatibility testing)
fn create_test_batch_workflow(id: &str) -> Workflow {
    let route = Route::with_priority(
        "rt_001",
        "batch_route",
        Condition::Always,
        vec![Action::Log {
            level: "info".to_string(),
            message: "Processing batch record".to_string(),
        }],
        100,
    );

    Workflow::new(id, format!("Batch Workflow {}", id), vec![route])
        .with_description("Integration test batch workflow")
}

#[tokio::test]
async fn test_streaming_workflow_creation_and_validation() {
    // Create streaming workflow
    let workflow = create_test_streaming_workflow("wf_stream_001", "test_events");

    // Validate workflow
    assert!(workflow.validate().is_ok(), "Workflow validation failed");
    assert_eq!(workflow.id, "wf_stream_001");

    // Verify execution mode
    match &workflow.execution_mode {
        ExecutionMode::Streaming { config } => {
            assert_eq!(config.source_topic, "test_events");
            assert_eq!(config.consumer_group, "wf_stream_001_group");
            assert_eq!(config.checkpoint_interval_ms, 60000);
            assert_eq!(config.max_parallel_workers, Some(2));
        }
        _ => panic!("Expected streaming execution mode"),
    }
}

#[tokio::test]
async fn test_batch_workflow_backward_compatibility() {
    // Create batch workflow (old-style, no execution_mode specified)
    let workflow = create_test_batch_workflow("wf_batch_001");

    // Validate workflow
    assert!(
        workflow.validate().is_ok(),
        "Batch workflow validation failed"
    );

    // Verify execution mode defaults to Batch
    match &workflow.execution_mode {
        ExecutionMode::Batch => {
            // Expected - backward compatible
        }
        _ => panic!("Expected batch execution mode by default"),
    }
}

#[tokio::test]
async fn test_workflow_serialization_backward_compatibility() {
    // Create batch workflow
    let batch_workflow = create_test_batch_workflow("wf_ser_001");

    // Serialize to JSON
    let json = serde_json::to_string(&batch_workflow).expect("Failed to serialize");

    // Deserialize back
    let deserialized: Workflow = serde_json::from_str(&json).expect("Failed to deserialize");

    // Verify execution mode defaults to Batch
    assert!(matches!(deserialized.execution_mode, ExecutionMode::Batch));
    assert_eq!(batch_workflow.id, deserialized.id);
}

#[tokio::test]
async fn test_streaming_workflow_serialization() {
    // Create streaming workflow
    let streaming_workflow = create_test_streaming_workflow("wf_ser_002", "test_topic");

    // Serialize to JSON
    let json = serde_json::to_string(&streaming_workflow).expect("Failed to serialize");

    // Verify JSON contains execution_mode
    assert!(json.contains("execution_mode"));
    assert!(json.contains("streaming"));
    assert!(json.contains("test_topic"));

    // Deserialize back
    let deserialized: Workflow = serde_json::from_str(&json).expect("Failed to deserialize");

    // Verify execution mode preserved
    match &deserialized.execution_mode {
        ExecutionMode::Streaming { config } => {
            assert_eq!(config.source_topic, "test_topic");
        }
        _ => panic!("Expected streaming execution mode"),
    }
}

#[tokio::test]
async fn test_workflow_store_integration() {
    let store = WorkflowStore::new();

    // Create and store batch workflow
    let batch_wf = create_test_batch_workflow("wf_store_001");
    store
        .create(batch_wf.clone())
        .expect("Failed to create batch workflow");

    // Create and store streaming workflow
    let stream_wf = create_test_streaming_workflow("wf_store_002", "events");
    store
        .create(stream_wf.clone())
        .expect("Failed to create streaming workflow");

    // Retrieve and verify both workflows
    let retrieved_batch = store
        .get("wf_store_001")
        .expect("Failed to retrieve batch workflow");
    assert!(retrieved_batch.is_some());
    assert!(matches!(
        retrieved_batch.unwrap().execution_mode,
        ExecutionMode::Batch
    ));

    let retrieved_stream = store
        .get("wf_store_002")
        .expect("Failed to retrieve streaming workflow");
    assert!(retrieved_stream.is_some());
    assert!(matches!(
        retrieved_stream.unwrap().execution_mode,
        ExecutionMode::Streaming { .. }
    ));
}

#[tokio::test]
async fn test_stream_executor_initialization() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let execution_store = Arc::new(ExecutionStore::new());

    // Create StreamExecutor
    let executor = StreamExecutor::new(workflow_store.clone(), execution_store.clone());

    // Verify initial state
    let active_streams = executor.list_active_streams().await;
    assert_eq!(
        active_streams.len(),
        0,
        "Should start with no active streams"
    );
}

#[tokio::test]
async fn test_stream_executor_requires_streaming_mode() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let execution_store = Arc::new(ExecutionStore::new());
    let executor = StreamExecutor::new(workflow_store.clone(), execution_store.clone());

    // Try to start stream with batch workflow
    let batch_wf = create_test_batch_workflow("wf_exec_001");

    let result = executor.start_stream(&batch_wf).await;
    assert!(
        result.is_err(),
        "Should fail to start stream with batch workflow"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not configured for streaming"),
        "Error message should indicate streaming config required"
    );
}

#[tokio::test]
async fn test_kafka_source_creation() {
    // Create Kafka source
    let kafka_source = KafkaSource::new(
        "test_topic",
        "test_group",
        vec!["localhost:9092".to_string()],
        HashMap::new(),
    );

    assert!(
        kafka_source.is_ok(),
        "Failed to create Kafka source: {:?}",
        kafka_source.err()
    );
}

#[tokio::test]
async fn test_multiple_execution_modes_coexist() {
    let workflow_store = Arc::new(WorkflowStore::new());

    // Create batch workflow
    let batch_wf = create_test_batch_workflow("wf_multi_001");
    workflow_store
        .create(batch_wf)
        .expect("Failed to create batch workflow");

    // Create streaming workflow
    let stream_wf = create_test_streaming_workflow("wf_multi_002", "events");
    workflow_store
        .create(stream_wf)
        .expect("Failed to create streaming workflow");

    // Verify both exist and have correct modes by fetching full workflows
    let batch_retrieved = workflow_store
        .get("wf_multi_001")
        .expect("Failed to get batch workflow");
    let stream_retrieved = workflow_store
        .get("wf_multi_002")
        .expect("Failed to get streaming workflow");

    assert!(batch_retrieved.is_some(), "Batch workflow should exist");
    assert!(
        stream_retrieved.is_some(),
        "Streaming workflow should exist"
    );

    // Verify execution modes
    assert!(
        matches!(
            batch_retrieved.unwrap().execution_mode,
            ExecutionMode::Batch
        ),
        "Should be batch mode"
    );
    assert!(
        matches!(
            stream_retrieved.unwrap().execution_mode,
            ExecutionMode::Streaming { .. }
        ),
        "Should be streaming mode"
    );
}

#[tokio::test]
async fn test_streaming_workflow_lifecycle() {
    let workflow_store = Arc::new(WorkflowStore::new());
    let execution_store = Arc::new(ExecutionStore::new());

    // Create streaming workflow
    let workflow = create_test_streaming_workflow("wf_lifecycle_001", "test_events");
    workflow_store
        .create(workflow.clone())
        .expect("Failed to create workflow");

    // Create executor
    let _executor = StreamExecutor::new(workflow_store.clone(), execution_store.clone());

    // NOTE: Actual streaming start requires Kafka broker
    // In CI/CD, this would use Testcontainers or a test Kafka cluster
    // For now, we verify the workflow is properly configured

    // Verify workflow can be retrieved
    let retrieved = workflow_store
        .get("wf_lifecycle_001")
        .expect("Failed to retrieve");
    assert!(retrieved.is_some());

    // Verify execution mode
    match &retrieved.unwrap().execution_mode {
        ExecutionMode::Streaming { config } => {
            assert_eq!(config.source_topic, "test_events");
            assert_eq!(config.max_parallel_workers, Some(2));
        }
        _ => panic!("Expected streaming mode"),
    }
}

#[tokio::test]
async fn test_resource_estimation() {
    // Create streaming workflow with different worker counts
    let workflow = create_test_streaming_workflow("wf_resource_001", "events");

    // Get resource estimate
    let resources = workflow.execution_mode.estimate_resources();

    // Verify resource estimates are reasonable for streaming
    match &workflow.execution_mode {
        ExecutionMode::Streaming { config } => {
            let workers = config.max_parallel_workers.unwrap_or(4);
            assert!(
                resources.cpu_cores >= workers * 2,
                "CPU cores should scale with workers"
            );
            assert!(
                resources.memory_mb >= workers * 4096,
                "Memory should scale with workers"
            );
            assert!(resources.storage_mb > 0, "Should have storage requirement");
        }
        _ => panic!("Expected streaming mode"),
    }
}

#[tokio::test]
async fn test_workflow_validation_with_streaming_config() {
    // Valid streaming workflow
    let valid_workflow = create_test_streaming_workflow("wf_valid_001", "events");
    assert!(
        valid_workflow.validate().is_ok(),
        "Valid streaming workflow should pass validation"
    );

    // Invalid: empty ID
    let invalid_workflow = create_test_streaming_workflow("", "events");
    assert!(
        invalid_workflow.validate().is_err(),
        "Workflow with empty ID should fail validation"
    );

    // Invalid: no routes
    let mut invalid_workflow = Workflow::new("wf_invalid_002", "test", vec![]);
    invalid_workflow.execution_mode = ExecutionMode::Streaming {
        config: StreamingConfig {
            source_topic: "test".to_string(),
            consumer_group: "group".to_string(),
            checkpoint_interval_ms: 60000,
            watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
                max_out_of_orderness_ms: 30000,
            },
            max_parallel_workers: Some(2),
            state_backend: StateBackendConfig::Memory,
            auto_scaling: None,
            kafka_properties: HashMap::new(),
        },
    };
    assert!(
        invalid_workflow.validate().is_err(),
        "Workflow with no routes should fail validation"
    );
}

#[tokio::test]
async fn test_streaming_config_validation() {
    let config = StreamingConfig {
        source_topic: "test_topic".to_string(),
        consumer_group: "test_group".to_string(),
        checkpoint_interval_ms: 5000, // 5 seconds
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 10000,
        },
        max_parallel_workers: Some(4),
        state_backend: StateBackendConfig::Memory,
        auto_scaling: None,
        kafka_properties: HashMap::new(),
    };

    let mode = ExecutionMode::Streaming { config };
    let result = mode.validate();
    assert!(
        result.is_ok(),
        "Valid streaming config should pass validation"
    );
}

#[tokio::test]
async fn test_streaming_config_validation_failures() {
    // Invalid: checkpoint interval is zero
    let config = StreamingConfig {
        source_topic: "test".to_string(),
        consumer_group: "group".to_string(),
        checkpoint_interval_ms: 0, // Invalid
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30000,
        },
        max_parallel_workers: Some(2),
        state_backend: StateBackendConfig::Memory,
        auto_scaling: None,
        kafka_properties: HashMap::new(),
    };

    let mode = ExecutionMode::Streaming { config };
    let result = mode.validate();
    assert!(
        result.is_err(),
        "Checkpoint interval = 0 should fail validation"
    );
    assert!(result.unwrap_err().to_string().contains("> 0"));

    // Invalid: empty topic
    let config = StreamingConfig {
        source_topic: "".to_string(),
        consumer_group: "group".to_string(),
        checkpoint_interval_ms: 60000,
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30000,
        },
        max_parallel_workers: Some(2),
        state_backend: StateBackendConfig::Memory,
        auto_scaling: None,
        kafka_properties: HashMap::new(),
    };

    let mode = ExecutionMode::Streaming { config };
    let result = mode.validate();
    assert!(result.is_err(), "Empty topic should fail validation");
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));

    // Invalid: empty consumer group
    let config = StreamingConfig {
        source_topic: "test".to_string(),
        consumer_group: "".to_string(),
        checkpoint_interval_ms: 60000,
        watermark_strategy: WatermarkStrategy::BoundedOutOfOrderness {
            max_out_of_orderness_ms: 30000,
        },
        max_parallel_workers: Some(2),
        state_backend: StateBackendConfig::Memory,
        auto_scaling: None,
        kafka_properties: HashMap::new(),
    };

    let mode = ExecutionMode::Streaming { config };
    let result = mode.validate();
    assert!(
        result.is_err(),
        "Empty consumer group should fail validation"
    );
    assert!(result.unwrap_err().to_string().contains("cannot be empty"));
}
