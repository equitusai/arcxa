//! Checkpoint Manager
//!
//! Orchestrates workflow state persistence between:
//! - Hot storage: RocksDB (coordinator)
//! - Durable storage: RDF triples (shard)
//!
//! Provides periodic checkpointing and recovery mechanisms.

use crate::workflows::domain::{ExecutionStatus, WorkflowExecution};
use crate::workflows::storage::{RocksExecutionStore, WorkflowShardClient};
use anyhow::{Context, Result};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Checkpoint configuration
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Checkpoint interval (seconds)
    pub interval_secs: u64,

    /// Enable automatic checkpointing
    pub enabled: bool,

    /// Checkpoint only running executions
    pub running_only: bool,

    /// Shard URL for remote storage
    pub shard_url: Option<String>,

    /// Stale execution timeout (seconds)
    /// Executions running longer than this will be marked as stale/failed
    /// Default: 3600 seconds (1 hour)
    pub stale_timeout_secs: u64,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            interval_secs: 30,
            enabled: true,
            running_only: true,
            shard_url: None,
            stale_timeout_secs: 3600, // 1 hour
        }
    }
}

/// Recovery report after coordinator restart
#[derive(Debug, Clone)]
pub struct RecoveryReport {
    /// Number of executions recovered
    pub recovered_count: usize,

    /// Number of executions marked as stale/failed
    pub stale_count: usize,

    /// Execution IDs that were recovered
    pub recovered_ids: Vec<String>,

    /// Execution IDs that were marked stale
    pub stale_ids: Vec<String>,

    /// Recovery source
    pub recovery_source: RecoverySource,
}

/// Source of recovery data
#[derive(Debug, Clone, PartialEq)]
pub enum RecoverySource {
    /// Recovered from local RocksDB
    LocalRocksDB,
    /// Recovered from shard RDF store
    ShardRDF,
    /// Recovered from both (validated consistency)
    Both,
}

/// Checkpoint manager
///
/// Coordinates state persistence between:
/// - Hot storage: RocksDB (coordinator)
/// - Durable storage: RDF triples (shard)
pub struct CheckpointManager {
    rocks_store: Arc<RocksExecutionStore>,
    shard_client: Arc<RwLock<Option<WorkflowShardClient>>>,
    config: CheckpointConfig,
    checkpoint_sequence: Arc<RwLock<u64>>,
}

impl CheckpointManager {
    /// Create a new checkpoint manager
    pub fn new(rocks_store: Arc<RocksExecutionStore>, config: CheckpointConfig) -> Self {
        Self {
            rocks_store,
            shard_client: Arc::new(RwLock::new(None)),
            config,
            checkpoint_sequence: Arc::new(RwLock::new(0)),
        }
    }

    /// Initialize shard client connection
    pub async fn connect_shard(&self) -> Result<()> {
        if let Some(ref shard_url) = self.config.shard_url {
            info!(
                "Connecting to shard for workflow persistence: {}",
                shard_url
            );

            let client = WorkflowShardClient::connect(shard_url)
                .await
                .with_context(|| format!("Failed to connect to shard at {}", shard_url))?;

            *self.shard_client.write().await = Some(client);

            info!("Successfully connected to shard");
        } else {
            info!("No shard URL configured - using local RocksDB only");
        }

        Ok(())
    }

    /// Start periodic checkpointing (runs in background)
    pub async fn start_periodic_checkpointing(self: Arc<Self>) {
        if !self.config.enabled {
            info!("Periodic checkpointing disabled");
            return;
        }

        info!(
            "Starting periodic checkpointing (interval: {}s)",
            self.config.interval_secs
        );

        let mut ticker = interval(Duration::from_secs(self.config.interval_secs));

        loop {
            ticker.tick().await;

            if let Err(e) = self.checkpoint_all().await {
                error!("Periodic checkpoint failed: {}", e);
            }
        }
    }

    /// Checkpoint all eligible executions
    pub async fn checkpoint_all(&self) -> Result<usize> {
        debug!("Running checkpoint for all eligible executions");

        let executions = if self.config.running_only {
            self.rocks_store
                .list_in_progress()
                .context("Failed to list in-progress executions")?
        } else {
            self.rocks_store
                .list_filtered(&Default::default(), None, None)
                .context("Failed to list all executions")?
        };

        let mut checkpoint_count = 0;

        for execution in executions {
            if let Err(e) = self.checkpoint_execution(&execution.execution_id).await {
                error!(
                    "Failed to checkpoint execution {}: {}",
                    execution.execution_id, e
                );
            } else {
                checkpoint_count += 1;
            }
        }

        debug!("Checkpointed {} executions", checkpoint_count);

        Ok(checkpoint_count)
    }

    /// Checkpoint a single execution
    pub async fn checkpoint_execution(&self, execution_id: &str) -> Result<()> {
        debug!("Checkpointing execution: {}", execution_id);

        // Get current execution state
        let execution = self.rocks_store.get_required(execution_id)?;

        // Create checkpoint in RocksDB
        self.rocks_store
            .checkpoint(execution_id)
            .context("Failed to create RocksDB checkpoint")?;

        // Async send checkpoint to shard if connected
        if let Some(ref mut client) = *self.shard_client.write().await {
            let sequence = self.next_sequence().await;

            match client.store_checkpoint(&execution, sequence).await {
                Ok(response) => {
                    debug!(
                        "Checkpoint stored in shard: {} ({} triples, {}ms)",
                        response.checkpoint_uri, response.triples_created, response.duration_ms
                    );
                }
                Err(e) => {
                    warn!("Failed to store checkpoint in shard: {}", e);
                    // Continue - we still have local RocksDB checkpoint
                }
            }
        }

        debug!("Checkpoint created for execution: {}", execution_id);

        Ok(())
    }

    /// Recover executions after coordinator restart
    pub async fn recover_on_startup(&self) -> Result<RecoveryReport> {
        info!("Starting execution recovery...");

        // Try to get in-progress executions from both sources
        let local_in_progress = self.get_local_in_progress()?;
        let shard_in_progress = self.get_shard_in_progress().await?;

        let (in_progress, recovery_source) = if !shard_in_progress.is_empty() {
            info!(
                "Recovering from shard ({} executions)",
                shard_in_progress.len()
            );
            (shard_in_progress, RecoverySource::ShardRDF)
        } else if !local_in_progress.is_empty() {
            info!(
                "Recovering from local RocksDB ({} executions)",
                local_in_progress.len()
            );
            (local_in_progress, RecoverySource::LocalRocksDB)
        } else {
            info!("No in-progress executions to recover");
            return Ok(RecoveryReport {
                recovered_count: 0,
                stale_count: 0,
                recovered_ids: vec![],
                stale_ids: vec![],
                recovery_source: RecoverySource::LocalRocksDB,
            });
        };

        let mut recovered_ids = Vec::new();
        let mut stale_ids = Vec::new();

        for execution in in_progress {
            // Check if execution is stale (started longer than configured timeout with no updates)
            let elapsed = chrono::Utc::now()
                .signed_duration_since(execution.started_at)
                .num_seconds();

            if elapsed > self.config.stale_timeout_secs as i64 {
                // Mark as stale/failed
                warn!(
                    "Marking stale execution as failed: {} (started {} seconds ago, timeout: {} seconds)",
                    execution.execution_id, elapsed, self.config.stale_timeout_secs
                );

                if let Err(e) = self.mark_execution_stale(&execution.execution_id).await {
                    error!(
                        "Failed to mark execution {} as stale: {}",
                        execution.execution_id, e
                    );
                } else {
                    stale_ids.push(execution.execution_id.clone());
                }
            } else {
                // Recover execution
                info!("Recovering execution: {}", execution.execution_id);
                recovered_ids.push(execution.execution_id.clone());
            }
        }

        let report = RecoveryReport {
            recovered_count: recovered_ids.len(),
            stale_count: stale_ids.len(),
            recovered_ids,
            stale_ids,
            recovery_source,
        };

        info!(
            "Recovery complete: {} recovered, {} marked stale (source: {:?})",
            report.recovered_count, report.stale_count, report.recovery_source
        );

        Ok(report)
    }

    /// Get in-progress executions from local RocksDB
    fn get_local_in_progress(&self) -> Result<Vec<WorkflowExecution>> {
        self.rocks_store.list_in_progress()
    }

    /// Get in-progress executions from shard
    async fn get_shard_in_progress(&self) -> Result<Vec<WorkflowExecution>> {
        if let Some(ref mut client) = *self.shard_client.write().await {
            // Query shard for in-progress execution IDs
            let exec_ids = client.query_in_progress_executions().await?;

            if exec_ids.is_empty() {
                return Ok(vec![]);
            }

            // Batch fetch checkpoints
            let checkpoints = client.batch_get_checkpoints(exec_ids).await?;

            // Convert to Vec
            Ok(checkpoints.into_values().collect())
        } else {
            Ok(vec![])
        }
    }

    /// Mark an execution as stale/failed
    async fn mark_execution_stale(&self, execution_id: &str) -> Result<()> {
        self.rocks_store
            .set_error(
                execution_id,
                "Execution marked as stale after coordinator restart".to_string(),
            )
            .context("Failed to mark execution as stale")?;

        self.rocks_store
            .update_status(execution_id, ExecutionStatus::Failed)
            .context("Failed to update execution status")?;

        // Also update in shard if connected
        if let Some(ref mut client) = *self.shard_client.write().await {
            let execution = self.rocks_store.get_required(execution_id)?;
            let sequence = self.next_sequence().await;

            if let Err(e) = client.store_checkpoint(&execution, sequence).await {
                warn!("Failed to update stale status in shard: {}", e);
            }
        }

        Ok(())
    }

    /// Get next checkpoint sequence number
    async fn next_sequence(&self) -> u64 {
        let mut seq = self.checkpoint_sequence.write().await;
        *seq += 1;
        *seq
    }

    /// Restore execution from latest checkpoint
    pub async fn restore_from_checkpoint(&self, execution_id: &str) -> Result<WorkflowExecution> {
        info!("Restoring execution from checkpoint: {}", execution_id);

        // Try shard first (most authoritative)
        if let Some(ref mut client) = *self.shard_client.write().await {
            if let Ok(Some(checkpoint)) = client.get_latest_checkpoint(execution_id).await {
                info!("Restored execution from shard checkpoint: {}", execution_id);
                return Ok(checkpoint);
            }
        }

        // Fall back to local RocksDB
        let checkpoint = self
            .rocks_store
            .get_latest_checkpoint(execution_id)
            .context("Failed to get latest checkpoint")?
            .ok_or_else(|| {
                anyhow::anyhow!("No checkpoint found for execution: {}", execution_id)
            })?;

        info!("Restored execution from local checkpoint: {}", execution_id);

        Ok(checkpoint)
    }

    /// Get recovery statistics
    pub async fn get_recovery_stats(&self) -> Result<RecoveryStats> {
        let total_executions = self.rocks_store.count()?;
        let in_progress = self.rocks_store.list_in_progress()?.len();

        Ok(RecoveryStats {
            total_executions,
            in_progress_executions: in_progress,
        })
    }

    /// Check if shard is connected
    pub async fn is_shard_connected(&self) -> bool {
        self.shard_client.read().await.is_some()
    }
}

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    pub total_executions: usize,
    pub in_progress_executions: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_execution(id: &str, workflow_id: &str) -> WorkflowExecution {
        WorkflowExecution::new(
            id.to_string(),
            workflow_id.to_string(),
            format!("Workflow {}", workflow_id),
            json!({"test": "data"}),
            Some("test@example.com".to_string()),
        )
    }

    #[tokio::test]
    async fn test_checkpoint_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksExecutionStore::open(temp_dir.path()).unwrap());

        let config = CheckpointConfig::default();
        let manager = CheckpointManager::new(store.clone(), config);

        assert!(!manager.is_shard_connected().await);
    }

    #[tokio::test]
    async fn test_checkpoint_single_execution() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksExecutionStore::open(temp_dir.path()).unwrap());

        let mut execution = create_test_execution("exec_001", "wf_001");
        execution.update_status(ExecutionStatus::Running);
        store.save(execution).unwrap();

        let config = CheckpointConfig::default();
        let manager = CheckpointManager::new(store.clone(), config);

        // Checkpoint the execution
        manager.checkpoint_execution("exec_001").await.unwrap();

        // Verify checkpoint exists in RocksDB
        let checkpoint = store.get_latest_checkpoint("exec_001").unwrap();
        assert!(checkpoint.is_some());
        assert_eq!(checkpoint.unwrap().execution_id, "exec_001");
    }

    #[tokio::test]
    async fn test_checkpoint_all() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksExecutionStore::open(temp_dir.path()).unwrap());

        // Create multiple running executions
        for i in 1..=3 {
            let mut exec = create_test_execution(&format!("exec_{:03}", i), "wf_001");
            exec.update_status(ExecutionStatus::Running);
            store.save(exec).unwrap();
        }

        let config = CheckpointConfig::default();
        let manager = CheckpointManager::new(store.clone(), config);

        // Checkpoint all
        let count = manager.checkpoint_all().await.unwrap();
        assert_eq!(count, 3);

        // Verify checkpoints exist
        for i in 1..=3 {
            let exec_id = format!("exec_{:03}", i);
            let checkpoint = store.get_latest_checkpoint(&exec_id).unwrap();
            assert!(checkpoint.is_some());
        }
    }

    #[tokio::test]
    async fn test_recovery_from_local() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksExecutionStore::open(temp_dir.path()).unwrap());

        // Create recent running execution
        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        store.save(exec1).unwrap();

        // Create old running execution (should be marked stale)
        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.started_at = chrono::Utc::now() - chrono::Duration::hours(2);
        exec2.update_status(ExecutionStatus::Running);
        store.save(exec2).unwrap();

        let config = CheckpointConfig::default();
        let manager = CheckpointManager::new(store.clone(), config);

        // Run recovery
        let report = manager.recover_on_startup().await.unwrap();

        assert_eq!(report.recovered_count, 1);
        assert_eq!(report.stale_count, 1);
        assert_eq!(report.recovered_ids[0], "exec_001");
        assert_eq!(report.stale_ids[0], "exec_002");
        assert_eq!(report.recovery_source, RecoverySource::LocalRocksDB);

        // Verify exec_002 is marked as failed
        let exec2_updated = store.get("exec_002").unwrap().unwrap();
        assert_eq!(exec2_updated.status, ExecutionStatus::Failed);
    }

    #[tokio::test]
    async fn test_recovery_stats() {
        let temp_dir = TempDir::new().unwrap();
        let store = Arc::new(RocksExecutionStore::open(temp_dir.path()).unwrap());

        // Create executions
        let mut exec1 = create_test_execution("exec_001", "wf_001");
        exec1.update_status(ExecutionStatus::Running);
        store.save(exec1).unwrap();

        let mut exec2 = create_test_execution("exec_002", "wf_001");
        exec2.update_status(ExecutionStatus::Completed);
        store.save(exec2).unwrap();

        let config = CheckpointConfig::default();
        let manager = CheckpointManager::new(store.clone(), config);

        let stats = manager.get_recovery_stats().await.unwrap();
        assert_eq!(stats.total_executions, 2);
        assert_eq!(stats.in_progress_executions, 1);
    }
}
