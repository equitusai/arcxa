//! Checkpoint manager with periodic writes and graceful shutdown

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Notify;

use super::{Checkpoint, CheckpointStorage};

/// Periodic checkpoint writer with shutdown support
pub struct CheckpointManager {
    storage: Arc<CheckpointStorage>,
    interval_secs: u64,
    shutdown_notify: Arc<Notify>,
}

impl CheckpointManager {
    pub fn new(storage: CheckpointStorage, interval_secs: u64) -> Self {
        Self {
            storage: Arc::new(storage),
            interval_secs,
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    /// Get shutdown notifier (call notify_one() to trigger graceful shutdown)
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        self.shutdown_notify.clone()
    }

    /// Start periodic checkpoint writer
    ///
    /// # Arguments
    /// - `kafka_offsets_fn`: Closure to get current Kafka offsets
    /// - `dedup_state_fn`: Closure to get current dedup state
    /// - `worker_count`: Number of workers (for validation on restore)
    ///
    /// # Returns
    /// Join handle to the checkpoint task
    ///
    /// # Shutdown
    /// Call `shutdown_handle().notify_one()` to trigger graceful shutdown.
    /// The task will write a final checkpoint before exiting.
    pub fn start<F1, F2>(
        self,
        kafka_offsets_fn: F1,
        dedup_state_fn: F2,
        worker_count: usize,
    ) -> tokio::task::JoinHandle<Result<()>>
    where
        F1: Fn() -> HashMap<i32, i64> + Send + Sync + 'static,
        F2: Fn() -> HashMap<String, i64> + Send + Sync + 'static,
    {
        let storage = self.storage.clone();
        let shutdown_notify = self.shutdown_notify.clone();

        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(self.interval_secs));

            tracing::info!(
                "Checkpoint manager started: interval={}s",
                self.interval_secs
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Periodic checkpoint
                        if let Err(e) = Self::write_checkpoint(
                            &storage,
                            &kafka_offsets_fn,
                            &dedup_state_fn,
                            worker_count
                        ).await {
                            tracing::error!("Failed to write periodic checkpoint: {}", e);
                            crate::ingestion::metrics::CHECKPOINT_ERRORS
                                .with_label_values(&["write_failed"]).inc();
                        }
                    }

                    _ = shutdown_notify.notified() => {
                        // Graceful shutdown - write final checkpoint
                        tracing::info!("Checkpoint manager shutting down, writing final checkpoint...");

                        if let Err(e) = Self::write_checkpoint(
                            &storage,
                            &kafka_offsets_fn,
                            &dedup_state_fn,
                            worker_count
                        ).await {
                            tracing::error!("Failed to write final checkpoint: {}", e);
                            crate::ingestion::metrics::CHECKPOINT_ERRORS
                                .with_label_values(&["shutdown_failed"]).inc();
                        } else {
                            tracing::info!("Final checkpoint written successfully");
                        }

                        return Ok(());
                    }
                }
            }
        })
    }

    async fn write_checkpoint<F1, F2>(
        storage: &Arc<CheckpointStorage>,
        kafka_offsets_fn: &F1,
        dedup_state_fn: &F2,
        worker_count: usize,
    ) -> Result<()>
    where
        F1: Fn() -> HashMap<i32, i64>,
        F2: Fn() -> HashMap<String, i64>,
    {
        let start = std::time::Instant::now();

        let checkpoint = Checkpoint::new(worker_count)
            .with_offsets(kafka_offsets_fn())
            .with_dedup_state(dedup_state_fn());

        let path = storage.write_async(checkpoint.clone()).await?;

        let duration = start.elapsed();
        tracing::info!(
            "Checkpoint written in {:?}: {:?} ({} KB, {} offsets, {} dedup entries)",
            duration,
            path.file_name().unwrap(),
            checkpoint.estimated_size() / 1024,
            checkpoint.kafka_offsets.len(),
            checkpoint.dedup_state.len()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_checkpoint_manager_periodic() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let counter = Arc::new(AtomicU64::new(0));
        let counter_clone = counter.clone();

        let manager = CheckpointManager::new(storage.clone(), 1); // 1 second interval
        let shutdown = manager.shutdown_handle();

        let handle = manager.start(
            move || {
                let mut offsets = HashMap::new();
                offsets.insert(0, counter_clone.fetch_add(1, Ordering::SeqCst) as i64);
                offsets
            },
            || HashMap::new(),
            4,
        );

        // Wait for 3 checkpoints
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Shutdown
        shutdown.notify_one();
        handle.await.unwrap().unwrap();

        // Verify checkpoints were written
        let checkpoints = storage.list().unwrap();
        assert!(checkpoints.len() >= 3);

        // Verify offsets increased
        let latest = storage.latest().unwrap().unwrap();
        assert!(latest.kafka_offsets.get(&0).unwrap() > &0);
    }

    #[tokio::test]
    async fn test_checkpoint_manager_shutdown() {
        let dir = tempdir().unwrap();
        let storage = CheckpointStorage::new(dir.path().to_path_buf()).unwrap();

        let manager = CheckpointManager::new(storage.clone(), 60); // 60 second interval
        let shutdown = manager.shutdown_handle();

        let handle = manager.start(
            || {
                let mut offsets = HashMap::new();
                offsets.insert(0, 999);
                offsets
            },
            || {
                let mut state = HashMap::new();
                state.insert("final_record".to_string(), 123456);
                state
            },
            4,
        );

        // Shutdown immediately (before periodic checkpoint)
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        shutdown.notify_one();
        handle.await.unwrap().unwrap();

        // Verify final checkpoint was written
        let latest = storage.latest().unwrap().unwrap();
        assert_eq!(latest.kafka_offsets.get(&0), Some(&999));
        assert_eq!(latest.dedup_state.get("final_record"), Some(&123456));
    }
}
