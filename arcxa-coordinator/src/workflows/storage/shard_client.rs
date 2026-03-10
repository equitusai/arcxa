//! Shard Client for Workflow State Persistence
//!
//! gRPC client for persisting workflow execution checkpoints to the shard as RDF triples.
//! Provides SPARQL-queryable workflow history and distributed recovery.

use crate::workflows::domain::{ExecutionStatus as DomainExecutionStatus, WorkflowExecution};
use anyhow::{Context, Result};
use std::time::Duration;
use tonic::transport::Channel;
use tracing::{debug, error, info, warn};

// Import generated protobuf code
pub mod proto {
    tonic::include_proto!("graphica.workflow.v1");
}

use proto::{
    workflow_state_service_client::WorkflowStateServiceClient, BatchGetCheckpointsRequest,
    BatchGetCheckpointsResponse, ExecutionStatus as ProtoExecutionStatus, GetCheckpointRequest,
    GetCheckpointResponse, QueryExecutionsRequest, QueryExecutionsResponse, StoreCheckpointRequest,
    StoreCheckpointResponse,
};

/// Shard client for workflow state operations
#[derive(Clone)]
pub struct WorkflowShardClient {
    client: WorkflowStateServiceClient<Channel>,
    default_graph: String,
}

impl WorkflowShardClient {
    /// Create a new workflow shard client
    pub async fn connect(shard_url: &str) -> Result<Self> {
        info!("Connecting to workflow shard at: {}", shard_url);

        let channel = Channel::from_shared(shard_url.to_string())
            .context("Invalid shard URL")?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .connect()
            .await
            .with_context(|| format!("Failed to connect to shard at {}", shard_url))?;

        let client = WorkflowStateServiceClient::new(channel);

        Ok(Self {
            client,
            default_graph: "http://graphica.io/workflow/executions".to_string(),
        })
    }

    /// Store a checkpoint to the shard
    pub async fn store_checkpoint(
        &mut self,
        execution: &WorkflowExecution,
        sequence: u64,
    ) -> Result<StoreCheckpointResponse> {
        debug!(
            "Storing checkpoint for execution: {} (sequence: {})",
            execution.execution_id, sequence
        );

        // Serialize execution state to JSON
        let state_json = serde_json::to_vec(execution).context("Failed to serialize execution")?;

        // Compress with LZ4
        let compressed_state = compress_lz4(&state_json)?;

        // Calculate checksum
        let checksum = calculate_sha256(&state_json);

        // Map domain status to proto status
        let proto_status = map_status_to_proto(&execution.status);

        let request = StoreCheckpointRequest {
            execution_id: execution.execution_id.clone(),
            sequence,
            workflow_id: execution.workflow_id.clone(),
            status: proto_status,
            step_number: 0, // TODO: Track step number in WorkflowExecution
            compressed_state,
            state_checksum: checksum,
            created_at: Some(prost_types::Timestamp {
                seconds: execution.started_at.timestamp(),
                nanos: execution.started_at.timestamp_subsec_nanos() as i32,
            }),
            metadata: std::collections::HashMap::new(),
            graph_uri: self.default_graph.clone(),
        };

        let response = self
            .client
            .store_checkpoint(request)
            .await
            .context("Failed to store checkpoint")?
            .into_inner();

        if !response.success {
            anyhow::bail!("Checkpoint storage failed: {}", response.error);
        }

        debug!(
            "Checkpoint stored: {} ({} triples, {}ms)",
            response.checkpoint_uri, response.triples_created, response.duration_ms
        );

        Ok(response)
    }

    /// Get the latest checkpoint for an execution
    pub async fn get_latest_checkpoint(
        &mut self,
        execution_id: &str,
    ) -> Result<Option<WorkflowExecution>> {
        debug!("Fetching latest checkpoint for execution: {}", execution_id);

        let request = GetCheckpointRequest {
            execution_id: execution_id.to_string(),
            sequence: 0, // 0 = latest
            include_state: true,
        };

        let response = self
            .client
            .get_latest_checkpoint(request)
            .await
            .context("Failed to get checkpoint")?
            .into_inner();

        if !response.found {
            return Ok(None);
        }

        let checkpoint = response
            .checkpoint
            .ok_or_else(|| anyhow::anyhow!("Checkpoint found but data missing"))?;

        // Decompress state
        let state_bytes = decompress_lz4(&checkpoint.compressed_state)?;

        // Deserialize execution
        let execution: WorkflowExecution = serde_json::from_slice(&state_bytes)
            .context("Failed to deserialize execution from checkpoint")?;

        debug!("Checkpoint retrieved for execution: {}", execution_id);

        Ok(Some(execution))
    }

    /// Batch get checkpoints for multiple executions (for recovery)
    pub async fn batch_get_checkpoints(
        &mut self,
        execution_ids: Vec<String>,
    ) -> Result<std::collections::HashMap<String, WorkflowExecution>> {
        info!(
            "Batch fetching checkpoints for {} executions",
            execution_ids.len()
        );

        let request = BatchGetCheckpointsRequest {
            execution_ids,
            latest_only: true,
            include_state: true,
        };

        let response = self
            .client
            .batch_get_checkpoints(request)
            .await
            .context("Failed to batch get checkpoints")?
            .into_inner();

        let mut result = std::collections::HashMap::new();

        for (exec_id, checkpoint_list) in response.checkpoints {
            if let Some(checkpoint) = checkpoint_list.checkpoints.first() {
                // Decompress and deserialize
                let state_bytes = decompress_lz4(&checkpoint.compressed_state)?;
                let execution: WorkflowExecution = serde_json::from_slice(&state_bytes)
                    .context("Failed to deserialize execution")?;

                result.insert(exec_id, execution);
            }
        }

        info!("Retrieved {} checkpoints from shard", result.len());

        Ok(result)
    }

    /// Query in-progress executions from shard (for recovery)
    pub async fn query_in_progress_executions(&mut self) -> Result<Vec<String>> {
        info!("Querying in-progress executions from shard");

        let request = QueryExecutionsRequest {
            sparql_where: String::new(), // Use status filter instead
            select_vars: vec!["execution_id".to_string()],
            limit: 1000,
            offset: 0,
            order_by: String::new(),
            time_range: None,
            status_filter: vec![proto::ExecutionStatus::Running as i32],
            include_checkpoints: false,
        };

        let mut stream = self
            .client
            .query_executions(request)
            .await
            .context("Failed to query executions")?
            .into_inner();

        let mut execution_ids = Vec::new();

        while let Some(response) = stream
            .message()
            .await
            .context("Failed to read query response")?
        {
            if let Some(proto::query_executions_response::Result::Execution(exec)) = response.result
            {
                execution_ids.push(exec.execution_id);
            }
        }

        info!(
            "Found {} in-progress executions in shard",
            execution_ids.len()
        );

        Ok(execution_ids)
    }

    /// Set default named graph for storage
    pub fn set_default_graph(&mut self, graph_uri: String) {
        self.default_graph = graph_uri;
    }
}

// Helper functions

fn map_status_to_proto(status: &DomainExecutionStatus) -> i32 {
    use proto::ExecutionStatus;
    match status {
        DomainExecutionStatus::Pending => ExecutionStatus::Pending as i32,
        DomainExecutionStatus::Running => ExecutionStatus::Running as i32,
        DomainExecutionStatus::Paused => ExecutionStatus::Paused as i32,
        DomainExecutionStatus::Completed => ExecutionStatus::Completed as i32,
        DomainExecutionStatus::Failed => ExecutionStatus::Failed as i32,
        DomainExecutionStatus::Stopped => ExecutionStatus::Cancelled as i32,
        DomainExecutionStatus::Aborted => ExecutionStatus::Cancelled as i32,
    }
}

fn compress_lz4(data: &[u8]) -> Result<Vec<u8>> {
    // Use lz4_flex for pure Rust LZ4 compression
    let compressed = lz4_flex::compress_prepend_size(data);
    Ok(compressed)
}

fn decompress_lz4(data: &[u8]) -> Result<Vec<u8>> {
    lz4_flex::decompress_size_prepended(data).context("Failed to decompress LZ4 data")
}

fn calculate_sha256(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        // Test with small data - round trip should work
        let small_data = b"Hello, World! This is a test of LZ4 compression.";
        let compressed = compress_lz4(small_data).unwrap();
        let decompressed = decompress_lz4(&compressed).unwrap();
        assert_eq!(small_data.as_ref(), decompressed.as_slice());

        // Test with larger, repetitive data - should actually compress
        let large_data = "AAAAAAAAAA".repeat(100);
        let compressed_large = compress_lz4(large_data.as_bytes()).unwrap();
        let decompressed_large = decompress_lz4(&compressed_large).unwrap();
        assert_eq!(large_data.as_bytes(), decompressed_large.as_slice());

        // Large repetitive data should compress significantly
        assert!(compressed_large.len() < large_data.len());
    }

    #[test]
    fn test_checksum() {
        let data = b"test data";
        let checksum = calculate_sha256(data);

        // SHA-256 of "test data" should be consistent
        assert_eq!(checksum.len(), 64); // SHA-256 is 256 bits = 64 hex chars
    }

    #[test]
    fn test_status_mapping() {
        use proto::ExecutionStatus;
        assert_eq!(
            map_status_to_proto(&DomainExecutionStatus::Running),
            ExecutionStatus::Running as i32
        );
        assert_eq!(
            map_status_to_proto(&DomainExecutionStatus::Completed),
            ExecutionStatus::Completed as i32
        );
    }
}
