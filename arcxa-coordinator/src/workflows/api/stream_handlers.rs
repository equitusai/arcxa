//! Streaming Workflow API Handlers
//!
//! HTTP request handlers for streaming workflow management.
//! These endpoints coexist with batch endpoints for backward compatibility.

use super::dto::*;
use crate::workflows::domain::{ExecutionMode, Workflow};
use crate::workflows::engine::{StreamExecutor, StreamHandle, StreamStats};
use crate::workflows::storage::{ExecutionStore, WorkflowStore};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Streaming API state
#[derive(Clone)]
pub struct StreamingApiState {
    pub workflow_store: Arc<WorkflowStore>,
    pub execution_store: Arc<ExecutionStore>,
    pub stream_executor: Arc<StreamExecutor>,
}

impl StreamingApiState {
    pub fn new(workflow_store: WorkflowStore, execution_store: ExecutionStore) -> Self {
        let workflow_store_arc = Arc::new(workflow_store);
        let execution_store_arc = Arc::new(execution_store);

        let stream_executor = Arc::new(StreamExecutor::new(
            workflow_store_arc.clone(),
            execution_store_arc.clone(),
        ));

        Self {
            workflow_store: workflow_store_arc,
            execution_store: execution_store_arc,
            stream_executor,
        }
    }
}

// === Request/Response DTOs ===

#[derive(Debug, Serialize, Deserialize)]
pub struct StartStreamRequest {
    /// Workflow ID to start streaming
    pub workflow_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StartStreamResponse {
    /// Workflow ID
    pub workflow_id: String,

    /// Number of workers
    pub workers: usize,

    /// Kafka consumer group
    pub consumer_group: String,

    /// Message
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StopStreamResponse {
    /// Workflow ID
    pub workflow_id: String,

    /// Message
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StreamStatsResponse {
    /// Workflow ID
    pub workflow_id: String,

    /// Total records processed
    pub records_processed: u64,

    /// Current throughput (records/sec)
    pub throughput: f64,

    /// Average latency (ms)
    pub avg_latency_ms: u64,

    /// Current lag (messages behind)
    pub lag: u64,

    /// Current watermark (event time)
    pub watermark: Option<chrono::DateTime<chrono::Utc>>,

    /// Active workers
    pub active_workers: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListStreamsResponse {
    /// Active streaming workflows
    pub streams: Vec<String>,

    /// Total count
    pub total: usize,
}

// === API Error Type ===

#[derive(Debug)]
pub struct StreamApiError {
    pub message: String,
    pub status: StatusCode,
}

impl IntoResponse for StreamApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": self.message
        }));
        (self.status, body).into_response()
    }
}

impl From<anyhow::Error> for StreamApiError {
    fn from(err: anyhow::Error) -> Self {
        error!("Streaming API error: {:?}", err);
        Self {
            message: err.to_string(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// === API Handlers ===

/// Start streaming execution for a workflow
///
/// POST /api/v1/workflows/{id}/stream/start
pub async fn start_stream(
    State(state): State<Arc<StreamingApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<StartStreamResponse>, StreamApiError> {
    info!("Starting streaming execution for workflow: {}", workflow_id);

    // Load workflow
    let workflow = state
        .workflow_store
        .get(&workflow_id)?
        .ok_or_else(|| anyhow::anyhow!("Workflow not found: {}", workflow_id))?;

    // Verify workflow is configured for streaming
    match &workflow.execution_mode {
        ExecutionMode::Streaming { config } => {
            info!(
                "Workflow {} is configured for streaming (topic: {}, group: {})",
                workflow_id, config.source_topic, config.consumer_group
            );
        }
        _ => {
            return Err(StreamApiError {
                message: format!(
                    "Workflow {} is not configured for streaming execution. Current mode: {:?}",
                    workflow_id, workflow.execution_mode
                ),
                status: StatusCode::BAD_REQUEST,
            });
        }
    }

    // Start stream
    let handle = state.stream_executor.start_stream(&workflow).await?;

    info!(
        "Streaming execution started for workflow: {} with {} workers",
        workflow_id, handle.workers
    );

    Ok(Json(StartStreamResponse {
        workflow_id: handle.workflow_id,
        workers: handle.workers,
        consumer_group: handle.consumer_group,
        message: format!(
            "Streaming execution started with {} workers",
            handle.workers
        ),
    }))
}

/// Stop streaming execution for a workflow
///
/// POST /api/v1/workflows/{id}/stream/stop
pub async fn stop_stream(
    State(state): State<Arc<StreamingApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<StopStreamResponse>, StreamApiError> {
    info!("Stopping streaming execution for workflow: {}", workflow_id);

    state.stream_executor.stop_stream(&workflow_id).await?;

    info!("Streaming execution stopped for workflow: {}", workflow_id);

    Ok(Json(StopStreamResponse {
        workflow_id: workflow_id.clone(),
        message: format!("Streaming execution stopped for workflow: {}", workflow_id),
    }))
}

/// Get streaming statistics for a workflow
///
/// GET /api/v1/workflows/{id}/stream/stats
pub async fn get_stream_stats(
    State(state): State<Arc<StreamingApiState>>,
    Path(workflow_id): Path<String>,
) -> Result<Json<StreamStatsResponse>, StreamApiError> {
    info!("Getting streaming stats for workflow: {}", workflow_id);

    let stats = state.stream_executor.get_stats(&workflow_id).await?;

    Ok(Json(StreamStatsResponse {
        workflow_id: workflow_id.clone(),
        records_processed: stats.records_processed,
        throughput: stats.throughput,
        avg_latency_ms: stats.avg_latency_ms,
        lag: stats.lag,
        watermark: stats.watermark,
        active_workers: stats.active_workers,
    }))
}

/// List all active streaming workflows
///
/// GET /api/v1/workflows/stream/active
pub async fn list_active_streams(
    State(state): State<Arc<StreamingApiState>>,
) -> Result<Json<ListStreamsResponse>, StreamApiError> {
    info!("Listing active streaming workflows");

    let streams = state.stream_executor.list_active_streams().await;
    let total = streams.len();

    Ok(Json(ListStreamsResponse { streams, total }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflows::domain::{
        Action, Condition, Route, StateBackendConfig, StreamingConfig, WatermarkStrategy,
    };
    use crate::workflows::storage::{ExecutionStore, WorkflowStore};
    use std::collections::HashMap;

    fn create_test_state() -> Arc<StreamingApiState> {
        let workflow_store = WorkflowStore::new();
        let execution_store = ExecutionStore::new();
        Arc::new(StreamingApiState::new(workflow_store, execution_store))
    }

    fn create_streaming_workflow(id: &str, name: &str) -> Workflow {
        let route = Route::with_priority(
            "rt_001",
            "test_route",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "Test".to_string(),
            }],
            10,
        );

        let mut workflow = Workflow::new(id, name, vec![route]);
        workflow.execution_mode = ExecutionMode::Streaming {
            config: StreamingConfig {
                source_topic: "test_topic".to_string(),
                consumer_group: "test_group".to_string(),
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

        workflow
    }

    #[tokio::test]
    async fn test_start_stream_requires_streaming_workflow() {
        let state = create_test_state();

        // Create non-streaming workflow
        let route = Route::with_priority(
            "rt_001",
            "test",
            Condition::Always,
            vec![Action::Log {
                level: "info".to_string(),
                message: "test".to_string(),
            }],
            10,
        );
        let workflow = Workflow::new("wf_001", "batch_workflow", vec![route]);
        state.workflow_store.create(workflow).unwrap();

        // Try to start stream
        let result = start_stream(State(state.clone()), Path("wf_001".to_string())).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        assert!(err.message.contains("not configured for streaming"));
    }

    #[tokio::test]
    async fn test_list_active_streams_empty() {
        let state = create_test_state();

        let result = list_active_streams(State(state)).await.unwrap();

        assert_eq!(result.0.total, 0);
        assert_eq!(result.0.streams.len(), 0);
    }

    #[tokio::test]
    async fn test_stop_nonexistent_stream() {
        let state = create_test_state();

        let result = stop_stream(State(state), Path("nonexistent".to_string())).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_stats_nonexistent_stream() {
        let state = create_test_state();

        let result = get_stream_stats(State(state), Path("nonexistent".to_string())).await;

        assert!(result.is_err());
    }
}
